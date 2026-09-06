# 0023 — Fields are plugin-modelled state over the experiment domain

Status: **accepted**  
Date: **2026-09-07**  
Refines: [ADR 0019](0019-kagami-experiment-document-model.md),
[ADR 0020](0020-compose-object-behaviour-through-plugin-components.md)

Refined by: [ADR 0024](0024-orishu-orchestrates-a-workload-component-graph.md)

## Context

Kagami authors particles and other discrete objects, but many experiments also
model quantities defined throughout a spatial domain: electromagnetic and
gravitational fields, or the velocity, density and pressure of a real medium.
Treating a field as an object component confuses domain state with the sources
and couplings carried by discrete objects. Treating a solver name as the field
identity makes scientifically different numerical models impossible to compare
or substitute deliberately.

Coulomb electrostatics and Maxwell/Yee electrodynamics compute the same
electromagnetic field family differently and couple to the same electric-charge
property, but cannot both govern that field in one experiment. Classical
gravity and gravitoelectromagnetism have the same relationship for the
gravitational field. A hydrodynamic plugin similarly evolves a medium field
rather than pretending every field sample is a discrete object.

## Decision

An experiment authors a spatial domain and selects the field families modeled
over that domain. A field is conceptually defined at every point in the domain;
its admitted numerical representation—grid, basis, particles, adaptive cells,
or another bounded discretization—is declared by the selected computational
model and workload profile.

A simulation plugin contributes one or more **computational models** for a
field family: declarative authoring and observation schemas, compatible source
and coupling component identities, initial/boundary-condition schemas,
dimensions and limits, plus the pinned workload code that updates the field.
The plugin/model owns the field's schema and state-transition semantics. Orishu
owns partitioning, accepted steps and the committed run boundary; it does not
reinterpret field values or choose a solver.

Exactly one computational model governs a field family in an experiment. Thus
Coulomb and Maxwell/Yee are mutually exclusive electromagnetic models, while
classical gravity and gravitoelectromagnetism are mutually exclusive
gravitational models. Different field families may coexist when their declared
components and execution phases are compatible.

Source and coupling properties are stable scientific vocabulary independent of
the selected compatible model. Electric charge couples an object to either
electromagnetic model; gravitational source/coupling mass couples it to either
gravity model. Switching models is an explicit validated authoring command and
never silently rewrites those object properties. Incompatible initial
conditions, boundary conditions, discretizations or observation requests block
the switch with structured diagnostics.

The logical observation at a committed boundary contains the modeled object
state and complete selected-field state. A simple client may request the
complete admitted field snapshot. Regional, channel and
level-of-detail subscriptions are transport/resource optimizations that return
an explicitly identified projection of that same boundary; they never change
the simulated field. Probes sample field values at fixed or object-attached
positions with dimensions, validity and provenance.

## Consequences

- The experiment model needs stable field-family and computational-model
  identities, mutually exclusive selection, and model-specific domain,
  discretization, initial-condition and boundary-condition configuration.
- X-PLUGIN packages more than a field name: they carry the schemas and pinned
  executable update method for each model they provide.
- K9 composition must connect object source/coupling components to selected
  field families without making a solver-specific charge or mass property.
- S-WORKLOAD and S-OBSERVE must represent complete logical field state and
  bounded projected/chunked transfer without requiring a monolithic allocation.
- Observer work may consume bounded resources, but cannot alter scientific
  state, participate in step commit or block simulation progress. An overloaded
  projection is shed, coalesced, reset, or disconnected outside the commit path.

## Non-goals

- Requiring the numerical representation to store a literal value for every
  mathematical point.
- Allowing two computational models to govern the same field family
  simultaneously.
- Treating presentation vectors, flow lines or probe glyphs as field state.

## Implementation

Tracked by [Define fields and computational-model selection](../tasks/kagami/define-fields-and-model-selection.md).
