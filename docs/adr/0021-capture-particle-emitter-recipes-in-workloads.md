# 0021 — Capture particle-emitter recipes in workloads

Status: **accepted**  
Date: **2026-09-07**  
Refines: [ADR 0008](0008-catalog-templates-instantiate-self-contained-objects.md),
[ADR 0010](0010-content-addressed-workload-closure-and-portable-bundles.md),
[ADR 0020](0020-compose-object-behaviour-through-plugin-components.md)

## Context

Particle emitters were a valuable Field CAD capability: an emitter could move
and respond to fields like another object while creating bounded particles from
one or more reusable definitions. Requiring Orishu to read Kagami's mutable
catalog at run time would violate workload closure and make results depend on a
client installation. Treating an emitter as a privileged object kind would
undermine component composition.

## Decision

A particle emitter is an ordinary modeled object carrying a plugin-contributed
emitter component. It may independently carry dynamics, field source, and field
coupling components, so the emitter itself can move or follow a field exactly as
any other object does.

When an emitter recipe is authored, its command selects one or more catalog
templates with explicit weights and materializes their complete spawn
blueprints into the experiment atomically. The emitter persists those
self-contained blueprints, their bindings, and source template fingerprints as
historical provenance—not live catalog references. It also declares bounded
emission parameters, including simulation-time rate or schedule, capacity,
direction/spread, initial velocity and optional lifetime. Plugin schemas may
extend that vocabulary while retaining declared dimensions and limits.

Workload compilation revalidates and copies each persisted **spawn blueprint**
into the immutable workload closure. It does not re-resolve the mutable catalog.
The blueprint contains the same complete component/property composition used to
instantiate an authored object, plus source provenance. Template names and
catalog paths do not reach the runtime. Orishu never consults Kagami's catalog,
and changing or losing a catalog after authoring cannot change or prevent
compilation of an otherwise compatible emitter.

The machinery has explicit stages. Catalog lookup and template binding remain
inside Kagami's catalog/authoring boundary. A pure, bounded blueprint
representation and validator are shared by authoring, workload compilation and
Orishu admission. The runtime converts an admitted blueprint into its hot entity
layout when spawning. No runtime code depends on catalog IO, catalog authority,
or Kagami's document representation.

Spawned objects are run state, not experiment revisions and not undo entries.
Emission is deterministic for a declared numerical profile: randomness derives
from workload/run identity, emitter identity, committed boundary and spawn
ordinal; checkpoints retain accumulator, counters and random-stream state.
Distributed execution assigns one fenced owner to an emitter at a boundary so
retries or repartitioning cannot duplicate a spawn.

Admission and execution bound blueprint bytes and nesting, template count,
emission rate, total/live spawned objects, lifetime, queueing and per-step work.
Capacity exhaustion has an explicit observation/diagnostic rather than silent
unbounded allocation.

## Consequences

- A catalog remains an authoring convenience while emitted objects remain
  reproducible without Kagami.
- The emitter showcases component composition: its own motion and its spawning
  behaviour have independent owners.
- Workload/checkpoint schemas need stable emitter, blueprint and spawn
  identities and deterministic restart semantics.
- The shared blueprint contract serves several consumers, but it must not
  become a second catalog authority or force authoring and simulation to share
  one in-memory layout.

## Non-goals

- Editing the experiment once for every runtime spawn.
- Fetching a template, plugin or mutable tag while a run is active.
- Defining collision, lifetime or rendering behaviour outside the components in
  each spawn blueprint.

## Implementation

Tracked by [Implement particle emitters](../tasks/kagami/implement-particle-emitters.md).
