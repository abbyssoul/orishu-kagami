# Implement particle emitters

Status: **specified**; cross-lane feature after composition and workload schema  
Work package: **X-EMITTER** ([roadmap](../../roadmap/README.md))  
Decision: [ADR 0021](../../adr/0021-capture-particle-emitter-recipes-in-workloads.md)

## Outcome

Researchers author an ordinary composed emitter that selects one or more
catalog templates, while Orishu executes only self-contained, bounded and
deterministic spawn blueprints captured in the workload.

## Slices

1. Define the plugin-contributed emitter schema: weighted recipes, rate/schedule,
   separately defined total/live capacity, direction/spread, initial velocity
   and optional lifetime.
2. Reuse K5's catalog lookup and binding path when an authoring command captures
   the complete transitive template/component/expression closure as
   self-contained spawn blueprints with stable identities, dimensions, schema
   versions and provenance. The command is atomic and undoable.
3. Define a sans-IO blueprint representation and validator shared by authoring,
   workload compilation and Orishu admission. Keep catalog IO and authority in
   Kagami; let Orishu convert an admitted blueprint into its own hot entity
   layout rather than sharing the document layout.
4. Implement deterministic single-node emission, lifecycle/despawn, diagnostics
   and checkpointed accumulator/counter/random-stream state.
5. Add fenced partition ownership, spawn identity and retry/repartition
   conformance so distributed execution neither loses nor duplicates spawns.
6. Publish spawned-object observations through S-OBSERVE for probes, following,
   field visualization and trails.

## Acceptance criteria

- The emitter itself can move under dynamics and field coupling independently
  of its spawning component.
- Multiple weighted templates are captured as self-contained blueprints in the
  experiment; changing or removing the source catalog after that authoring
  command cannot affect the experiment, compilation or a run.
- Orishu rejects catalog paths, unresolved templates, excessive nesting/rates/
  capacities and incompatible component schemas at admission.
- Equal accepted inputs produce equal spawn identities and state across restart,
  retry and supported partition changes.
- Spawned objects are run state and never create document revisions or undo.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- A privileged emitter object kind or mutable runtime catalog.
- Requiring trails to complete emitter simulation.
