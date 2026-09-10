# Choose the scene scale

Status: **implemented**; `kagami_session::default_view::SceneScale`,
scale-relative camera bounds, `defaultView` section version 2, and the view
control. The prefix-selecting formatter named in slice 3 is *not* here and
remains [S-VARIABLES](../migrate-and-integrate-variables-subsystem.md)' to
place  
Work package: **K-VIEW** ([roadmap](../../roadmap/README.md))  
Decision: [ADR 0022](../../adr/0022-persist-default-view-outside-experiment-intent.md)  
Milestone: **M2 — Authoring and admission foundations**; independent of worker
formation and run delivery  
Story: [Choose the scene scale](../../user-stories/kagami/authoring.md#choose-the-scene-scale)

## Outcome

One camera serves an atomic-scale and an astronomical-scale experiment. The
researcher says how many metres a viewport unit represents; the viewport's
reach, framing and reported distances follow, and nothing about the experiment
changes.

## Why this is not optional

K11 slice 2 landed camera bounds as absolute constants —
`MIN_CAMERA_DISTANCE` of 1 m, `MAX_CAMERA_DISTANCE` of 2 km, and a
`MAX_CAMERA_TARGET` of 1e30 m chosen only to keep an `f32` narrowing finite.
Those numbers make Kagami a viewer of room-sized scenes. A 2 nm molecule
cannot be approached, because the camera stops a metre away from it; a
planetary orbit cannot be framed, because the camera cannot retreat past two
kilometres. Neither limit is a considered scientific choice; both are
prototype constants that a scale factor is the correct way to remove.

Field CAD hit this exactly, and its own regression test says so:
`nanometre_scale_focus_does_not_collapse_to_the_minimum_distance` in
`../field-cad/apps/fieldcad-desktop/src/camera.rs` records that a
nanometre object's radius "collapses to ~1e-9 and clamps to `MIN_DISTANCE` —
the camera cannot approach any closer, regardless of how far a user scrolls
in."

## Owning boundary

The scale is presentation. It belongs in the client-owned default view
(`kagami_session::default_view`), converts at the rendering boundary
(`apps/kagami/src/viewport.rs` and `kagami-renderer`), and is invisible to
`kagami-document`, to workload compilation and to Orishu. No solver, no
expression and no persisted physical quantity learns about it.

## Source assessment

Inspected reference: `../field-cad/crates/fieldcad-core/src/scene_scale.rs`.
Its tests demonstrate the intended unit conversion, not Kagami's complete
validation contract. Retain these mechanics deliberately:

- A validated positive, finite length; a default of exactly 1.0 m per unit, so
  a scene that never touches the setting renders as though the setting did not
  exist.
- Named presets from nanometre to light-year, plus a directly entered
  metres-per-unit that parses a unit-bearing string (`1nm`). A value matching
  no preset presents as "Custom" rather than snapping.
- **Divide before the cast.** Compute `world_metres / metres_per_unit` in
  `f64`, check the result, then narrow to `f32`. The inverse multiplies in
  `f64` and must also be checked. Scaling puts a scene into the camera's usable
  numeric range; it does **not** improve the ratio between a tiny separation
  and a large absolute coordinate. Small details far from the origin can still
  need camera-relative rendering. Do not copy the reference's unchecked casts.
- The module's own warning, worth repeating verbatim in the Kagami type: this
  is "not a second way to store a position".

Four things do **not** transfer, and each is easy to reintroduce by accident:

1. **It is not experiment intent.** Field CAD held `scene_scale` in its world
   document and changed it through a world command
   (`CommandPayload::SetSceneScale`). Under ADR 0022 a setting that "never
   changes a stored object position, size, or physical constant" — Field CAD's
   own words, in its own tooltip — must not enter the experiment revision,
   undo history or workload identity. In Kagami it advances the *view*
   revision and dirties the file, exactly as projection does. This is the same
   divergence already recorded for `SetObjectVisible` under ADR 0012.
2. **It is outside MCP parity.** Because Field CAD made it a world command, it
   appears in `fieldcad-mcp`. ADR 0006 and ADR 0022 keep presentation controls
   out of the shared surface; the scale is per-window, like the camera.
3. **`glam` does not come with it.** Field CAD's conversion helpers return
   `glam::Vec3`/`DVec3`. `kagami-session` forbids `glam`
   (`crates/kagami-session/tests/dependencies.rs`) for the reason
   `kagami_document::geometry` states: a persisted contract must not depend on
   a renderer's representation. The scale carries a scalar conversion; the
   vector form belongs to the renderer's own boundary.
4. **No egui.** The preset picker and the drag-value field are rebuilt in Iced.

## Implementation slices

### 1. The scale as a bounded value

- `SceneScale` in `kagami_session::default_view`: a validated positive finite
  `f64` of metres per unit, `Default` of exactly 1.0, and checked scalar
  conversions. Refuse zero, negative and
  non-finite — unlike the camera pose's out-of-range clamping, a scale of zero
  names no mapping at all and there is nothing to clamp it towards.
- Positive and finite alone is insufficient: subnormal scales, `f64::MAX`,
  and finite coordinates can overflow or underflow in derived bounds or
  conversion. Define and document a supported scale interval covering every
  preset, with finite, nonzero derived camera distances and enough matrix
  headroom. Refuse values outside it with a structured reason; do not silently
  clamp a requested scale. Validate constructors and deserialization alike.
- Named presets nanometre through light-year, and a `label` that reports
  "Custom" for anything else, so the control cannot claim a preset it is not
  on.
- Add it to `AuthoringView`. This advances `DEFAULT_VIEW_VERSION` to 2 and
  leaves the envelope's `FORMAT_VERSION` alone — the first real use of the
  separate section version K11 slice 2 introduced for exactly this. Decoding a
  version-1 section yields the default scale, which is the behaviour a file
  written before this slice should have.
- Decode the whole versioned view DTO before applying scale-dependent camera
  validation. Do not deserialize its pose through today's metre-only clamps
  first: that would already destroy a nanometre camera distance. Missing or
  invalid scale in a version-2 section follows the existing nonblocking
  warning/default-view policy; a version-1 section explicitly supplies 1 m.
  Preserve the file-envelope v1/v2 compatibility and unsupported-primary
  refusal established by K4/K11.

### 2. Scale-relative camera bounds

The load-bearing slice, and the one with an API consequence worth naming in
advance: **the pose can no longer be validated alone.**

- Express `MIN_CAMERA_DISTANCE`, `MAX_CAMERA_DISTANCE` and the renderability
  limit in *render units*, and derive the metre-space bound from the scale. The
  stored pose stays canonical SI (a pose stored in render units would silently
  move in world space when the scale changed, and would mean nothing on its
  own).
- Consequently the validated unit becomes the view, including its pose and
  scale. Keep this invariant behind private fields/validated mutation methods;
  no public field assignment may install an incompatible pair. A raw pose DTO
  can still perform scale-independent finite checks. All load, motion and
  scale-change paths use the same view validation rules.
- `MAX_CAMERA_TARGET` stops being an absolute 1e30 m and becomes a bound on
  *render-space* magnitude, with headroom for matrix arithmetic. At
  astronomical-unit scale a 1e11 m target becomes order-one render coordinates;
  this fits the camera's working range but is not an origin-rebasing scheme.
  Retire the absolute metre-space bound, not the safety check it provided.
- Keep the guarantees K11's review established: a saved pose is still bounded
  on decode, the narrowing at
  `apps/kagami/src/viewport.rs` still cannot emit a non-finite value, and
  `Camera::view_matrix` still derives its direction analytically rather than by
  subtraction.

#### What happens when the user changes scale

- Preserve the SI orbit target, yaw, pitch and projection. Preserve SI orbit
  distance when it fits the new scale-relative interval; otherwise clamp it to
  the nearest distance limit and report the camera adjustment. Do not multiply
  the old SI pose by the scale ratio or implicitly fit the domain.
- If the unchanged target cannot be represented safely at the requested
  scale, refuse the change and explain that the view must be retargeted first;
  do not silently move its focus. Compute and validate the whole candidate
  before adopting anything. An accepted scale-plus-distance adjustment is one
  view revision; a refusal, or choosing the current scale, changes none.
- The existing save acknowledgement covers that exact combined view revision.
  New/Open must still protect unsaved view changes. Observation/replay uses the
  same validation on its ephemeral view, never on the retained authoring view.

### 3. The view control

- A preset picker reporting the active scale, and a distance-per-unit field
  that accepts a unit-bearing entry.
  `orishu_variables` already has the length unit table this needs — `m`, `km`,
  `mm`, `um`, `nm`, `pm`, `fm`, `AU` — plus `lookup` and full expression
  parsing, so *reading* `1 nm` needs no new parser. Use caller-supplied limits
  for source, parse and evaluation work, resolve only shared units/constants,
  and require a length result. Do not bind the setting to document/catalog
  variables or observations. A bare number in the explicitly labelled
  metres-per-unit field means metres; a wrong dimension is refused. Accepted
  input becomes a scalar scale, not a retained expression dependency.
- Keep formatting bounded to this feature: named preset labels and the
  existing canonical-SI quantity display for custom values suffice. A general
  prefix-selecting formatter is optional follow-up in S-VARIABLES, not a K14
  dependency or a reason to extract an app-only helper. The shared unit table
  currently has `AU` but no `ly`; use the explicit light-year preset value
  (9.4607304725808e15 m). Add a typed `ly` alias only through the shared unit
  table with its own tests, never a second private unit table/parser.
- Route the change through the client-local queue, mode-aware exactly as
  projection is: an authoring change advances the view revision and dirties the
  file, an Observation/replay change is ephemeral.
- State the active scale in the viewport, and keep every distance the UI
  *reports* in real units. A scale that is in effect but invisible is how
  someone misreads a scene by twelve orders of magnitude.

### 4. Rendering contract and integration proof

- Apply the same SI-to-render mapping to camera target/distance and every
  physical position/extent consumed by a renderer. Keep near/far planes,
  orthographic extents, grid/axis geometry and pointer motion consistently in
  render units, with checked inverse conversion for world-space distances and
  any existing picking path. Do not scale dimensionless directions or angles.
- Out-of-range geometry is omitted/clipped with a defined diagnostic policy,
  never saturated onto a fabricated boundary position. The inverse must not
  produce a non-finite authored candidate. No per-frame parsing or per-object
  heap allocation is introduced by the mapping.
- The current production renderer draws a grid and axes, not document objects
  or observations. Prove physical geometry/camera coherence with deterministic
  synthetic geometry through the production conversion and projection paths,
  including both projections. Extend the offscreen smoke fixture as needed;
  do not implement object rendering, picking, fields, trails or run transport
  merely to finish K14. Record that end-to-end object inspection still depends
  on those consumers adopting this mapping.

### Constraints on later consumers

- Where the scale seeds a default extent for a newly added object, that value
  becomes ordinary authored intent through the ordinary command path at
  creation, and a later scale change does not revisit it. The scale picks a
  starting number, the way `Model::next_object_number` picks a starting label.
  This is not permission to override an explicit catalog/template value. Adding
  scale-aware creation defaults is not required for K14's completion.
- Field glyph lengths and flow-line seeding (K11 slice 4) carry their own
  physical-magnitude scaling. Do not conflate the two: a vector's length
  encodes a field magnitude, and the scene scale encodes distance. One control
  must not quietly rescale the other's meaning.
- Grid and axis spacing are generated in render space and should read as real
  distances under the active scale.

## Implementation record

`kagami_session::default_view::SceneScale` is the metres-per-unit factor;
`AuthoringView` became the validated unit that owns it, its projection and its
pose together; the `defaultView` section advanced to version 2 while the file's
`FORMAT_VERSION` stayed at 2; and `apps/kagami/src/viewport.rs` divides before
it narrows. 32 `default_view` tests, 33 in `tests/document.rs`, 6 renderer
camera tests, 6 viewport boundary/integration tests, and 30 headless app
tests.

Twelve decisions are worth carrying forward. Four of them are corrections
review found, and each names the test that now holds the line:

- **A scale change refuses rather than relocating the focus.** The first
  version reused the clamping constructor for `with_scale`, so switching a
  camera focused a metre out to nanometre scale *succeeded* and slid the focus
  to a millimetre out — leaving someone looking at a different place with
  nothing to tell them. The focus is what the user is looking at, so it is
  never moved to make a scale fit: `ViewError::FocusOutOfReach` refuses, names
  the remedy ("bring the focus nearer the origin first"), and leaves the view
  and the revision untouched.

  The distinction that makes this work is `FocusPolicy`. **Refuse** for an
  explicit choice; **Clamp** for a decoded file, where ADR 0022 forbids a
  camera stopping a load, and for a gesture, where a pan held against the limit
  must stop rather than error. Both answers are right for their caller, and
  writing them as one policy parameter is what keeps that legible.
  (`an_incompatible_scale_is_refused_rather_than_moving_the_focus`,
  `a_file_and_a_gesture_still_clamp_the_focus_rather_than_refusing`)
- **An automatic distance adjustment is reported.** Selecting nanometre scale
  takes the default camera from eighteen metres to two micrometres, and the
  first version said nothing at all. `ViewAdjustment` now carries what moved,
  `ViewChange` pairs it with whether anything changed, and the wording lives on
  the session type so **both** workspace modes report it identically — a camera
  moved twelve orders of magnitude is no less confusing while watching a run.
  Gestures are deliberately exempt: a notice per frame of a held drag is noise,
  and hitting a limit is what a limit feels like.
  (`an_automatic_distance_adjustment_is_reported`,
  `an_adjustment_is_reported_while_observing_too`)
- **Accepted is not the same question as changed, nor as "left a notice".**
  Making `set_scale` report adjustments briefly broke the typed-entry field,
  because `update` read "notice present" as "refused" and kept the field open
  over a change that had worked. `Document::set_scale` returns *accepted*, and
  a scale already in force is accepted too — there is nothing to correct.
  (`an_unreachable_focus_refuses_the_scale_and_keeps_the_typed_entry`)
- **Orthographic projection needed a depth test, not a `w` test.** Under
  perspective a behind-eye point has negative `w`, so checking `w` catches it.
  Under orthographic `w` stays at one wherever the point is, so a point well
  behind the camera divided cleanly and produced plausible coordinates.
  `Camera::project` now checks NDC `z` against the declared `[0, 1]` DirectX
  convention, which is what makes the answer right for both — and the test
  sweeps both projections rather than the one that happened to work.
  (`a_point_behind_the_eye_is_omitted_under_both_projections`)

And the eight from the original implementation:

- **The validated unit is the view, as the slice predicted.** Whether a camera
  is reachable depends on the scale beside it, so the two cannot be set
  independently and stay consistent. `AuthoringView`'s fields are private and
  its constructors re-bound the pose; `CameraPose` keeps only what is true at
  *any* scale — finite components, a strictly positive distance, a pitch where
  the pan basis is defined. Public fields would have made an inconsistent pair
  constructible, which is precisely the state a renderer cannot draw.
- **Motion split into a raw step and a bound.** `CameraPose::stepped` produces
  what the gesture asked for and `AuthoringView::moved` brings it inside the
  scale's window. That split is what keeps every gesture *total*: a dolly that
  would drive the distance to zero clamps instead of failing, so a scroll wheel
  held against the stop is a no-op rather than an error. It also means a
  `CameraPose` never briefly holds a distance its own constructor would refuse.
- **`MAX_CAMERA_TARGET` became a precision bound, not a magnitude one.** In
  render units the limit says something true at every scale — 1e6 units is a
  thousand kilometres at metre scale and a million astronomical units at AU
  scale — and it is set by what `f32` can *resolve*, not by what it can hold.
  The old 1e30 metres only kept a cast finite and said nothing about either
  extreme.
- **Pan needed no scale at all.** It is already a fraction of the orbit
  distance, so it moves nanometres near a molecule and astronomical units near
  an orbit without either number appearing anywhere.
  `a_pan_step_follows_the_scale_without_referring_to_it` pins that, because it
  is the kind of property a later change would break silently.
- **A scale is refused, not clamped.** Every camera bound here clamps, and this
  one does not: a scale is a *ratio*, so zero names no mapping at all and a
  negative one mirrors the world. There is no nearest sensible value to bring
  either towards. That asymmetry is stated on `ViewError::ScaleNotPositive`
  because it otherwise looks like an inconsistency.
- **Reading `1 nm` needed no parser.** `orishu_variables` already treats a unit
  as part of the language, so `SceneScale::parse` evaluates through the one
  engine and gets arithmetic (`2 m / 2`) for free. A bare number is metres —
  the one convenience worth having in a metres-per-unit field — and any *other*
  dimension is refused as a mistake rather than taken as a shorthand.
- **Both conversions are fallible, and geometry is omitted rather than
  pinned.** `to_world` returns `Option` because it produces *authored
  candidates* — a picked position, a measured distance — and an authored
  coordinate is never allowed to be non-finite. `to_render` returns `Option`
  for the mirror-image reason review found: an accepted scale of 1e-30 turned a
  finite `f64::MAX` into infinity with no way to say so. That is the **geometry**
  direction, and geometry with no render-space answer is *dropped*, because an
  object drawn at the edge of the reachable region is indistinguishable from
  one that is really there. `viewport::geometry_units` is that path;
  `camera_units` is the separate, saturating one, because a camera must always
  render *something* and there is no "omit the viewport".
- **A finite positive ratio can still be unsupported.** `from_metres` first
  rejected only zero, negatives and non-finites — which left 1e-320 metres per
  unit accepted, where dividing an ordinary coordinate by the scale overflows
  to infinity. `MIN_SCENE_SCALE`/`MAX_SCENE_SCALE` bound it to 1e-30..1e30
  metres, five orders below the Planck length to ten thousand universe radii,
  so nothing physical is excluded and both conversions stay computable across
  the whole reachable range. It is a separate refusal from "not a ratio",
  because "a mistake" and "a limit" want different messages.

### The rendering contract, and what is proven versus deferred

`Camera::project` was added so a point can be asked where it lands without the
caller adopting `glam`. It returns `None` for a point behind the eye, on the
near plane, or through a matrix arithmetic has made non-finite — which is what
lets out-of-range geometry be *omitted* rather than saturated onto a boundary
position it does not occupy. Near/far planes, orthographic extents and grid and
axis spacing were already in render units and needed no change; they are now
correct at every physical scale rather than only near one metre. The mapping
introduces no parsing and no allocation per frame or per object:
`SceneScale::PRESETS` is a `'static` slice and preset names come from
`label()`, so the picker offering them every frame allocates nothing.

The proof is on **synthetic geometry through the production path**, since the
production renderer still draws only a grid and axes.
`the_same_structure_viewed_at_its_own_scale_lands_in_the_same_place` puts a
one-unit feature through metres → render units → `f32` → view-projection → NDC
at every preset under both projections, and asserts it occupies the same
fraction of the screen; `a_two_nanometre_feature_is_invisible_without_a_scale_and_visible_with_one`
states the failure as a comparison, showing the same 2 nm offset collapsing to
under 1e-9 NDC at metre scale and reaching 0.13 at nanometre scale.
`make smoke-kagami` now also sweeps the camera across the full reachable
render-unit window under both projections.

**End-to-end object inspection still depends on later consumers adopting this
mapping.** Objects, fields, trails and picking are not rendered yet; when they
are, each must convert through `SceneScale` at the same boundary rather than
casting metres to `f32` directly. Nothing here implements object rendering to
prove that, and nothing here should.

### What is deliberately not here

- **The prefix-selecting formatter.** `Display for Quantity` still prints a
  nanometre as `1e-9 m`, so the metres-per-unit field shows the raw value while
  the *label* carries the readable name. Placing a shared `format_si_value`
  equivalent is [S-VARIABLES](../migrate-and-integrate-variables-subsystem.md)'
  call, since the inspector wants it too; K14 did not grow a private one.
- **Scale-aware creation defaults.** Nothing seeds a default extent from the
  scale, because `CreateObject` currently authors no shape at all. The
  constraint below still binds whoever adds one.
- **A view-controls panel.** The picker and the typed field are in the settings
  panel and the active scale is on the toolbar. That split is a judgement about
  a 36-pixel toolbar that is already at capacity, and it has not been checked
  by eye — see the verification note.

## Acceptance criteria

- A 2 nm-radius geometry fixture at nanometre scale and an AU-scale orbit
  fixture at astronomical-unit scale occupy useful, nonzero screen extents in
  both projections. Equivalent dimensionless fixtures frame alike within
  stated floating-point tolerances. Use fixtures near the origin; this is not
  a claim about arbitrary distant-origin detail.
- Zero, negative and non-finite scales are refused with a reason. A scale
  matching no preset reports as custom.
- Test unsupported finite extremes, wrong dimensions, unknown symbols,
  over-budget input, checked conversion failure and inverse round-trip error.
  Accepted scale changes preserve target/orientation, adjust distance only as
  specified, and advance at most one view revision; refused/no-op changes do
  not dirty or partially change the view.
- Changing the scale advances the default-view revision and marks the file
  modified; it never advances the experiment revision, never adds an undo
  entry, and never alters an authored position, extent, velocity, expression or
  constant.
- The scale round-trips through the file. A document written before this slice
  loads with the default scale, through the section's own version conversion,
  and the envelope's `FORMAT_VERSION` does not move.
- Observation/replay scale changes dirty nothing and do not travel with a run
  reference; the scale is absent from the MCP surface.
- No non-finite value reaches a matrix at any supported scale, including the
  extremes of the preset range with the camera at both distance limits.
- File fixtures cover absent view, version-1 view migration and version-2
  nanometre/astronomical camera round trips without legacy metre clamping;
  malformed/unknown view versions still warn without blocking the experiment.
- Renderer tests plus `make smoke-kagami` are run where graphics hardware is
  available; any manual verification is recorded.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

### Verification recorded

- `make fmt-check`, `make lint`, `make test`, `make docs` and `make docs-check`
  all exit 0. 57 test suites, no failures.
- `make smoke-kagami` renders 120 offscreen frames — half under each
  projection, sweeping the camera across the full reachable render-unit window
  — on `Intel(R) Iris(R) Xe Graphics (RPL-P)` / Mesa 25.2.8 / Vulkan.
- `cargo run -p kagami -- --exit-after 4` launches and self-exits cleanly.
- Fixtures: `experiment_view_nanometre.json` records a 20 nm camera
  (`distance: 2e-8`, `scale: 1e-9`) and `experiment_view_astronomical.json` a
  20 AU one, both at section version 2 with the envelope still at
  `formatVersion: 2`. `experiment_view_v1.json` is the retained version-1
  section the conversion is checked against.
- **Not done by hand:** no interactive pass. Nobody has clicked a preset,
  typed `1 nm`, or seen where the control sits — so the placement decision
  (picker and typed field in the settings panel, active scale on the toolbar)
  is unverified by eye, and the toolbar was already near its width limit
  before this added a readout. The behaviour is asserted headlessly; the
  layout is a judgement call awaiting a look.

## Non-goals

- Per-object or per-layer scale, and any non-uniform or logarithmic scaling.
- Automatic scale selection from experiment contents. A camera that retargets
  itself by twelve orders of magnitude without being asked is worse than one
  that needs telling; an explicit "fit to domain" action is K11's, not this.
- Camera-relative world rendering. A scale factor addresses magnitude; the
  remaining precision loss of a distant origin is a separate change to the
  renderer's stage. It remains follow-up precision work for K-VIEW before
  promising inspection of arbitrarily small details at astronomical offsets;
  scaling alone must not be advertised as satisfying that broader story.
- Changing how any physical quantity is stored, parsed, or compiled. Canonical
  SI stays canonical SI.
