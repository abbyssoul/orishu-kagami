# Migration strategy

The previous repositories are reference implementations, not directories to
copy wholesale. Code enters this monorepo only when its role and interface are
clear in the combined product.

## Initial adoption

| Source | Decision | Destination |
| --- | --- | --- |
| Orishu shared models and client | Reuse as the client seam for Orishu-facing applications. | `libs/orishu` |
| Orishu worker and operator tools | Reuse and maintain as produced binaries. | `apps/orishu-*` |
| Orishu workload contract | Adopt the lifecycle and responsibility split, but make its implicit security boundary explicit: submitted physics is an untrusted WebAssembly Component with a closed capability ABI; native/Python equivalence is not inherited. | `docs/protocol-workload.md` and ADR 0009 |
| Orishu runtime goals and scaling plan | Retain scaling as a runtime-platform objective covering compute, capacity, storage, recovery, and hostile-input cost. Restore staged evidence and the 10,000-worker research target without presenting either as implemented. | `docs/orishu-scaling-objectives.md` and the roadmap |
| Orishu formation, node identity, and synthetic cluster resource | Retain the distinction between non-unique labels, formation identity, and cluster-assigned membership identity. | ADR 0013 and `docs/orishu-runtime-design.md` |
| Orishu artifact storage and deletion | Retain record/inventory/availability authority levels, QUIC-native verified transfer, and purge tombstones; rewrite them against content-addressed workload closure and current security rules. | ADRs 0014-0015, `docs/storage-model.md`, and `docs/storage-spec.md` |
| Orishu output provenance | Retain committed artifact provenance as authoritative and step/audit history as diagnostic. Extend it with current component, schema, observation, and execution-profile identity. | `docs/orishu-provenance.md` |
| Historical runtime proposals | Preserve live role changes, mDNS admission, replication changes, API compaction, and storage open questions as deferred design, not current behavior. | `docs/orishu-runtime-future-work.md` |
| Field CAD/Orishu Maxwell profile | Retain the narrow profile only as a proposal requiring revalidation because Kagami supersedes Field CAD and submission ownership changed. | ADR 0016 |
| Kagami Iced application shell | Reuse as the basis of the native client. | `apps/kagami` |
| Kagami Iced/wgpu scene renderer | Reuse as a deep rendering module. | `libs/kagami-renderer` |
| Kagami demo scene tree | Keep app-local until an authoritative experiment model replaces it. | `apps/kagami/src/scene_model.rs` |
| Kagami GPUI experiment | Do not adopt; it duplicates the shell and has a conflicting graphics graph. | Historical reference only |
| Field CAD egui desktop | Do not adopt; Kagami replaces this presentation implementation. | Historical reference only |
| Field CAD server and MCP transport | Do not adopt as a second compute/control plane; Orishu owns remote execution. | Rebuild against Orishu where needed |
| Field CAD expression and variable prototypes | Evaluate together; do not adopt either historical API wholesale. `fieldcad-variables` informs the generic namespaced dependency engine, while `fieldcad-expressions` informs dimension-aware resource integration, retained source, diagnostics, and bounds. | The shared `orishu-variables` subsystem conforming to ADRs 0005 and 0007 |
| Field CAD object catalog | Reuse its failure-isolation, availability, fingerprint, provenance, safe-write, and explicit-update lessons; replace Field CAD core types, resolved-only values, document-scoped sources, and tracking links with Kagami schemas, shared expressions, a client-owned catalog authority, and snapshot instantiation. | Kagami catalog domain conforming to ADR 0008 |

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
   neither UI state nor cluster runtime state. A kernel is implementation used
   to build a workload component; it is not by itself the installable
   simulation plugin or the authoring schema Kagami exposes.
4. **Visualization algorithms.** Port interpolation, glyph, trajectory, picking,
   and gizmo behaviour into Kagami's renderer only when the required observation
   interface exists.
5. **Catalog and physical schemas.** Adapt `fieldcad-catalog` behind Kagami's
   client-owned catalog authority. Reconcile stable identifiers, component
   schemas, expressions, SI units, fingerprints, and provenance before
   adopting stored formats. Persist complete instantiated objects rather than
   catalog dependencies or live tracking links.
6. **Variables and expressions.** Combine the useful seams demonstrated by
   Field CAD's `fieldcad-variables` and `fieldcad-expressions`: stable
   namespaced identities and dependency evaluation beneath a dimension-aware
   experiment and workload-resource layer. Preserve authored source, validate
   affected dependencies atomically, and use the same evaluator in Kagami and
   Orishu workload admission. Do not introduce live solver observations as an
   implicit document input.

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
