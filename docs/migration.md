# Migration strategy

The previous repositories are reference implementations, not directories to
copy wholesale. Code enters this monorepo only when its role and interface are
clear in the combined product.

## Initial adoption

| Source | Decision | Destination |
| --- | --- | --- |
| Orishu shared models and client | Reuse as the client seam for Orishu-facing applications. | `libs/orishu` |
| Orishu worker and operator tools | Reuse and maintain as produced binaries. | `apps/orishu-*` |
| Kagami Iced application shell | Reuse as the basis of the native client. | `apps/kagami` |
| Kagami Iced/wgpu scene renderer | Reuse as a deep rendering module. | `libs/kagami-renderer` |
| Kagami demo scene tree | Keep app-local until an authoritative experiment model replaces it. | `apps/kagami/src/scene_model.rs` |
| Kagami GPUI experiment | Do not adopt; it duplicates the shell and has a conflicting graphics graph. | Historical reference only |
| Field CAD egui desktop | Do not adopt; Kagami replaces this presentation implementation. | Historical reference only |
| Field CAD server and MCP transport | Do not adopt as a second compute/control plane; Orishu owns remote execution. | Rebuild against Orishu where needed |

## Field CAD extraction candidates

The following work is valuable, but each item needs a deliberate interface and
tests before migration:

1. **Experiment model and scene document.** Reconcile Field CAD's editable world
   with Orishu workload and artifact terminology, then define compilation from
   experiment intent to immutable workload input. Do not expose UI-framework or
   solver-owned types.
2. **Observation model.** Generalize immutable field snapshots into typed,
   versioned observations with provenance, completeness, validity, and
   backpressure rules suitable for the Orishu client protocol.
3. **Numerical kernels.** Move independently verified headless kernels below
   both the Kagami client and Orishu workload adapters. A kernel must depend on
   neither UI state nor cluster runtime state.
4. **Visualization algorithms.** Port interpolation, glyph, trajectory, picking,
   and gizmo behaviour into Kagami's renderer only when the required observation
   interface exists.
5. **Catalog and physical schemas.** Reconcile stable identifiers, SI units, and
   provenance with workload-package inputs before adopting stored formats.

## Admission test

A migrated module must pass all of these checks:

- Its name describes a role in Orishu Kagami rather than its source repository.
- Its interface is smaller and more stable than its implementation.
- Deleting it would force real complexity into multiple callers; otherwise it
  should remain inside its only caller.
- Tests exercise the same interface used by production callers.
- Persisted or networked data is versioned, bounded, and documented.
- The module does not create a second authority for workload or simulation
  state.
