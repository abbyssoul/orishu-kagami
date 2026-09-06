# Implement Kagami viewport workflows

Status: **specified**; modes/projection gated on K4/K6, field/follow on run
observations, live trails on V-LIVE, and exact trails on K-OBSERVATION/V-REPLAY  
Work package: **K-VIEW** ([roadmap](../../roadmap/README.md))  
Decision: [ADR 0022](../../adr/0022-persist-default-view-outside-experiment-intent.md)

## Outcome

Kagami clearly separates Authoring from Observation/replay and supplies the
Field CAD viewport capabilities that make scientific state legible:
perspective/orthographic projection, object follow, field vectors and flow
lines, plus bounded best-effort live trails and exact retained trajectories.

## Slices

1. Add an explicit workspace-mode state machine. Starting/opening a run enters
   Observation/replay; Edit initial conditions returns to Authoring, stopping a
   local preview or detaching from (not stopping) a remote run. Gate UI and MCP
   authoring adapters on that explicit transition without coupling run state
   into the document authority.
2. Add orthographic projection to `kagami-renderer`, expose both projections in
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

## Non-goals

- Shared cameras, presenter-follow or synchronized playback.
- Selection/gizmo completion or arbitrary visualization plugin code.
