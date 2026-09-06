# Model requested observations

Status: **specified**; gated on [K4](persist-experiment-documents.md) and the
X-PLUGIN observation-channel schema contract; K7's probe story is satisfied  
Work package: **K-DOCUMENT** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0004](../../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0012](../../adr/0012-start-with-file-sharing-and-preserve-collaborative-authoring.md),
[ADR 0019](../../adr/0019-kagami-experiment-document-model.md)

## Outcome

An experiment can declare *what it wants measured*: non-perturbing point probes, distances
between objects, aggregate quantities over a selection, exact retained object
trajectories, and the surfaces and regions a field should be sampled across. The Kagami glossary already includes
"requested observations" in the definition of an experiment; this task makes
them representable.

This is the model slice deliberately deferred out of K1 so the first document
model could land small.

## The distinction this task exists to preserve

A requested observation is **authored intent**: it is part of the experiment,
travels with the file, and is compiled into the submitted workload. A
*subscription* is **transport**: which channels a particular client currently
wants, at what density and level of detail, renewed after a reconnect. Field
CAD's story inventory separates them (its "Authored world" and "Transport
subscription" rows), ADR 0012 puts subscriptions in presentation state, and
collapsing them would make one client's viewport settings part of another
researcher's experiment.

Concretely: *"record the electric field at this point, attached to this
object"* is authored. *"send me that channel at stride 4 while I am looking at
it"* is not.

For fields, the authored request defines scientific capture requirements such
as channels, durable cadence/retention and the maximum region/resolution the
run must make available. A client subscription may request the whole admitted
field snapshot or a bounded regional/channel/LOD projection. Presentation then
chooses glyph density and flow-line seeds without changing either field state or
the authored capture contract.

These instruments are distinct from modeled objects. A physical sensor or
detector intended to perturb the simulation must be authored as a composed
object; an observation instrument cannot carry components or contribute to a
solve.

## Source assessment

`../field-cad/crates/fieldcad-core/src/world.rs` implemented all of these and
the shapes transfer, minus the divergences ADR 0019 records:

- **Probe** — a world position *or* an attachment to an object with a local
  offset, plus the declared channels it records. Field CAD's rule that channels
  stay selectable while their field system is inactive is worth keeping: a
  recorder configuration should survive a model change.
- **Distance probe** — two object identities; the live distance is derived,
  never stored.
- **Mass-aggregate probe** — a selection (all objects, or an explicit set) with
  a derived centroid. Field CAD also created a hidden anchor object so other
  instruments could attach to the centroid; decide whether Kagami needs that
  indirection or whether attachment can name the probe directly, and record the
  choice.
- **Slice plane, box, sphere** — bounded, orientable sampling geometry, each
  optionally attached to an object so it follows the object's transform.
- Resolution helpers (`resolve_probe_position`, `resolve_plane_frame`, …) are
  pure functions of a snapshot and transfer directly.

Not transferred: per-instrument `visible` flags and display options (line
drawing, member lines, vector mode) — presentation, per ADR 0012; history
buffers and recorded readings — run observations, per ADR 0004; and Field CAD's
`ObjectShape`-agnostic emitter machinery, which is a physics-source feature and
needs its own decision.

## Implementation slices

### 1. Instrument identities and model

- Add `ProbeId`, `DistanceProbeId`, `AggregateProbeId`, `RegionId` with their
  own monotonic counters, sharing K1's never-rewound rule.
- Add a bounded trajectory request keyed by stable object identities, cadence,
  duration/retention and requested position/velocity channels. It authors what
  a run retains, not an in-document history buffer.
- Store instruments in their own ordered collections behind the same shared
  pointer, so an edit touching only probes shares every other collection.
- An instrument is not a physical source and can never contribute to a solve.
  Make that structural, not a comment: instruments carry no components.

### 2. Commands and referential integrity

- Create, rename, reposition/reconfigure, and remove commands for each
  instrument kind, plus channel selection for probes.
- Removing an object referenced by an attached probe, a distance probe, or an
  attached region is rejected with its dependants identified, unless the same
  batch clears those references — the same rule K2 applies to a referenced
  variable. Field CAD's US-12 required this explicitly: no dangling
  relationship is silently retained.
- Dedicated rename commands never delete-and-recreate, so identities and
  attachments survive a rename (Field CAD US-18).

### 3. Channels without a plugin gate

- A channel identity is declared by a simulation plugin's schema. A probe may
  name a channel whose plugin is not currently enabled; that is reported as
  unavailable, not rejected, so disabling a model does not silently discard a
  recorder configuration.
- Bound the channel count per probe and the instrument counts per experiment in
  K1's `Limits`.
- Use the channel identity and compatibility representation owned by X-PLUGIN;
  do not introduce a K-DOCUMENT-private string that a later workload compiler
  must translate heuristically.

### 4. Persistence and projection

- Add instruments through the explicit experiment-format version policy from
  K4. A field may default to an empty collection only when that absence is the
  defined meaning for the source version; otherwise advance the format and add
  a conversion fixture.
- The read projection groups instruments separately from simulated objects and
  labels them as not simulated, so the scene tree cannot suggest a probe
  participates in physics.

## Acceptance criteria

- Every instrument kind can be created, renamed, reconfigured and removed as
  ordinary document commands, each one revision and one undo entry.
- An attached probe follows its object's transform through a derived
  resolution; no instrument stores a duplicated pose.
- Removing a referenced object is refused with its dependent instruments named;
  clearing the references in the same batch succeeds.
- A probe naming a channel from a disabled or uninstalled plugin is preserved
  and reported unavailable, never dropped or defaulted.
- No instrument carries components, contributes to a solve, or appears in the
  workload as a physical source.
- No visibility flag, display option, sampling stride, or level-of-detail
  setting appears in the document.
- Documents saved before this slice load unchanged; documents saved after it
  round-trip instrument identities and counters.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Recorded readings, history buffers, plots, or trajectory samples — run
  observations (ADR 0004, V-LIVE, V-REPLAY). This task stores only the request
  and retention contract for an exact trajectory.
- Client subscriptions, sampling density, level of detail, or channel
  visibility — presentation (ADR 0012).
- Particle emitters and other physics sources; they compose from plugin
  components and need their own decision.
- Compiling instruments into the workload's observation request; that follows
  the workload format (S-WORKLOAD) and is tracked by
  [K10](compile-and-query-observation-instruments.md).
