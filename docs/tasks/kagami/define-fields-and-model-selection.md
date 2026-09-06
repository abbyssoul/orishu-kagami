# Define fields and computational-model selection

Status: **specified**; required before field-capable authoring or execution  
Work package: **X-FIELDS** ([roadmap](../../roadmap/README.md))  
Decision: [ADR 0023](../../adr/0023-fields-are-plugin-modelled-domain-state.md)

## Outcome

An experiment defines a domain, selects field families within it, and chooses
exactly one plugin-contributed computational model for each family. Stable
source/coupling properties work across compatible models, while each model
provides its own field update code, numerical representation and constraints.

## Slices

1. Define stable `FieldFamilyId` and `ComputationalModelId` types and plugin
   schemas for compatible source/coupling components, dimensions, domain and
   discretization requirements, initial/boundary conditions, observation
   channels and limits.
2. Extend the experiment document with field-family/model selection and typed
   configuration commands. Enforce one model per family and preserve unavailable
   model intent under the existing schema-availability rules.
3. Define explicit compatibility/migration checks for switches such as Coulomb
   to Maxwell/Yee or classical gravity to gravitoelectromagnetism. Never rewrite
   charge or mass properties implicitly.
4. Map the selection and field initial state into S-WORKLOAD, then independently
   validate it during Orishu admission.
5. Define S-OBSERVE field snapshots as complete logical state with bounded,
   chunkable full-domain and regional/channel/LOD projections.
6. Add fixtures for electromagnetic and gravitational alternative models plus a
   hydrodynamic medium field, including mutual-exclusion and incompatibility
   failures.

## Acceptance criteria

- An experiment can select multiple compatible field families but exactly one
  computational model for each family.
- Coulomb and Maxwell/Yee consume the same stable electric-charge vocabulary;
  classical gravity and GEM consume the same stable gravitational coupling
  vocabulary, without simultaneous governance of their respective fields.
- Model changes validate domain, discretization, initial/boundary conditions,
  object components and requested observations atomically.
- A simple observation request can obtain complete field state for the admitted
  domain/resolution; projected subscriptions remain identifiable subsets of the
  same committed boundary.
- Missing plugins preserve authored field/model configuration as unavailable and
  block compilation without corrupting unrelated intent.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Implementing ADR 0024's executable component host or distributed placement;
  this task supplies the field/model identities and channel contracts it consumes.
- Mandating one grid or numerical method for all field families.
- Renderer vector, flow-line or probe styling; K-VIEW owns presentation.
