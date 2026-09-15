# Define and implement the shared plugin contract

Roadmap ID: **X-PLUGIN**. Status: **Concrete contract accepted 2026-09-16;
slice 1 ready; implementation and executable conformance evidence pending**.

The [accepted concrete v1 contract](../plugin-contract-v1-draft.md) settles R1–R8.
Produce section 11's conformance artifacts in the owning implementation slices;
they are not a reason to block the pure shared contract. Design approval is complete,
but executable specification closure and runtime integration are not yet complete.

## Outcome and authority

Researchers install immutable bundles, compose contributions from independent
providers, select scientific models explicitly, and export only the complete
selected execution closure. Kagami and Orishu consume one shared contract.

Follow [ADR 0027](../adr/0027-plugin-contributions-and-immutable-releases.md),
[plugin design and command draft](../simulation-plugins.md),
[authoring stories](../user-stories/kagami/authoring.md), and
[ADR 0024](../adr/0024-orishu-orchestrates-a-workload-component-graph.md).
ADR 0027 records accepted decisions; the gates below are not authorization to
invent the remaining persisted formats or compatibility policies in code.

## Verified current gap

Source inspected on 2026-09-12:

- `crates/kagami-document/src/setup.rs` stores `PluginComposition` as a set of
  logical catalog `PluginId`s, not immutable releases/contribution selections.
- `crates/kagami-catalog/src/schema.rs` owns a component-keyed `SchemaRegistry`
  whose insertion replaces a schema. It is not a provider-aware plugin inventory.
- `crates/kagami-document/src/capability.rs` supplies structural capability checks,
  not the accepted exact scientific-contract compatibility protocol.
- `crates/orishu-workload/src/ids.rs` and catalog types already have identifiers;
  the workload crate has bounded graph/canonical closure machinery. Neither is
  proof that plugin release identity, packaging or compilation exists.
- There is no `crates/orishu-plugin`. Do not duplicate existing identities or
  change canonical workload bytes without an explicit ownership/version decision.
- `crates/orishu-workload/src/graph.rs` exposes `ComponentInstance` with a list
  of roles. These are existing API/format names, not proof of the new one-execution-
  contract-per-kernel rule. Reconcile role/phase metadata with a kernel's contract
  identity explicitly; do not assume that merely renaming a type enforces it.

Recheck callers, tests and current implementation before beginning each slice.

## Slice 0 — Finish the contract

M1 design decisions below are resolved by the accepted v1 contract. Retain this
checklist for traceability; schemas, bindings and fixtures are remaining engineering
deliverables. Reopen a decision only for a concrete contradiction, not because the
implementation has not yet been written:

1. Specify provider eligibility and the concrete lock representation under the
   accepted policy: reuse existing exact pins, auto-resolve an unbound dependency
   only with one eligible provider, ask on ambiguity, and persist the selection.
   Define migration and missing-pin diagnostics without automatic substitution.
   Workload export must finish resolution; cluster admission never selects a
   provider. Headless authoring returns a structured ambiguity outcome with
   eligible choices; the caller explicitly selects and retries. Specify its
   bounded wire representation, not a separate headless resolution policy.
2. Freeze initial extension-point IDs, typed payloads, versions, units/dimensions,
   dependency declarations and diagnostics. Define scientific semantic hash
   projection and same-name/version/different-digest behavior; shape is not identity.
   The [initial field-to-entity flow](../simulation-plugins.md#field-to-entity-execution)
   now places field evolution and force production in the field kernel, followed
   by Dynamics reduction/integration. No separate coupling kernel is required for
   this profile. The first-pass pipeline is fixed: all field/force computation,
   then Dynamics reduction/integration; configurable schedules are future work.
   Specify force time/stage before freezing signatures; retain the existing
   static-without-Dynamics rule. Field `init` constructs kernel-defined natural
   defaults and bounded setup without coupled entities; validate the completed
   experiment separately. Local sandbox construction on field creation and capture
   of scientific output in the experiment are accepted. Reopen/submission preserve
   that state; runtime-only setup is separate. Specify concrete exports,
   configuration inputs and storage formats, not a source-dependent initialization
   policy or implicit reconstruction of scientific defaults on load.
3. Choose canonical release encoding, hashing and bounded decoding; specify
   descriptor closure, unknown opaque payload verification, self-reference rules,
   selected contribution provenance and enforcement of the accepted independent
   kernel artifact boundary (one execution-contract implementation per kernel). Reuse
   workload primitives where appropriate without assuming its codec is selected.
4. Choose a safe local archive/profile and inventory persistence/transaction
   contract, including concurrent CLI processes and effects on open documents.
   Freeze required command syntax, machine-readable outcomes and failure behavior.
5. Define logical plugin naming/ownership, human version/default selection and
   local source/provenance behavior. MVP distribution is now local-bundle-only;
   registry/discovery/remote updates and publisher signatures are post-launch
   scope, not initial gates. Specify explicit local-bundle update syntax;
   local support must not imply trusted publication or automatic network updates.
6. Publish an acyclic type/dependency ownership map for variables, catalog,
   workload and the planned plugin crate, with document/catalog/workload version
   and migration consequences. Coordinate X-FIELDS/X-COMPOSITION/O-WASM payloads
   and selected-closure compilation rather than creating a parallel model.
   Canonical prose uses kernel/kernel instance; map legacy component IDs, roles,
   lifecycle and wire fields explicitly. Specify Orishu-owned field-update and
   dynamics-integrator execution contracts with O-WASM, distinct from plugin-owned
   scientific vocabulary. Existing canonical bytes must not change implicitly.

Exit evidence: accepted decisions, example manifests and dependency fixtures,
canonical identity vectors, structured failure cases, ownership map, version
policy, and bounded acceptance tests specified for the next slice. Unresolved
later distribution features must have explicit follow-up scope and cannot
silently become requirements of the local MVP.

## Implementation slices after their design gates

### Ready handoff — slice 1 only

Implement the pure `orishu-plugin` contract against accepted sections 2–4, 9's
structured diagnostics/limits and 10's ownership map. Inspect current shared types
and canonical helpers first. Deliver release/contribution/contract identities,
root and six typed payload schemas, exact dependency references, bounded decoding,
canonical release/scientific hashing and structured validation errors. Publish
machine-readable fixtures and real golden bytes/digests, including independent
vocabulary/model providers and malformed/duplicate/oversized/unknown payload cases.
Test semantic-versus-presentation hashing and enforce the crate dependency boundary.
Reuse existing workload digest/canonical primitives through narrowly reviewed public
helpers; preserve existing workload v2 golden bytes and avoid a copied codec.

Do not implement resolver policy (slice 2), archive/inventory IO or commands (slice 3),
document migration, workloads with new selection evidence, or guest execution in
this handoff. Declare execution-contract references without pretending WIT bindings
exist. Where prose does not fully spell a field layout, document the concrete schema
and fixtures in the change; surface semantic ambiguity instead of inventing policy.
Run focused public serialized-path tests, dependency tests and relevant repository
checks. Report slice 1 evidence, not “X-PLUGIN complete”.

Slice 2 follows these public types. O-WASM's WIT/binding and execution-budget
fixtures, K4's container version audit and S-WORKLOAD's profile/version fixtures
gate their respective integration slices, not slice 1. X-EMITTER must reconcile
its implementation with the accepted newborn boundary schedule. The artifact-admin
security follow-up and post-launch registry work do not block this local MVP.

### Programme sequence

1. **Pure shared contract.** Add `orishu-plugin` with approved identities,
   envelopes/payloads, canonical encoding and bounded public decoders/validators.
   Enforce dependency constraints. No filesystem, network, runtime or UI access.
2. **Contribution availability and resolution.** Pure deterministic inventory
   projection with enablement, default/pinned releases, exact contracts, opaque
   unsupported payloads, bounded transitive dependencies/cycle diagnostics and
   explicit ambiguous choices. Keep install, available, selected and executable
   states separate. Use the slice-0 resolution policy, not map insertion order.
3. **Local packaging and inventory shell.** External-build inputs; validate/pack/
   inspect and safe atomic local install/list/set-default/update/remove/enable/
   disable through a headless authority. Apply process-only startup overrides;
   preserve old releases. Inspect Wasm without executing it; coordinate the
   inspection profile with O-WASM. Remote sources remain separately gated.
4. **Authoring integration.** Catalog/schema projection and document commands
   retain exact selected providers/releases/contracts, with explicit compatibility
   outcomes and migrations. Unavailable contributions preserve authored data.
   Resolve the current logical `PluginComposition` representation through the
   approved format migration, not a serde rename. UI/MCP use the same authority.
5. **Workload integration.** With S-WORKLOAD/X-COMPOSITION/X-FIELDS, compile the
   selected transitive closure and independently validate it at admission. Preserve
   graph/state ownership and dynamics integration semantics. No worker plugin
   installer and no dependency on Kagami inventory/catalog files at execution.
6. **Product adapters and external-author journey.** Generic host-owned authoring
   controls, inventory UI/MCP parity, CLI documentation and an external plugin
   example using the same validation path as built-ins. No custom visual extension
   system. Track any deferred remote publication/management in a bounded follow-up.

M1 owns specification/pure contract gates; M2 owns inventory and authoring/admission
integration. Actual simulation proof also depends on O-WASM/O-RUNTIME and the
scientific model tasks. Later publication UX does not block a local contract
fixture, but a fixture must not be reported as a working physics plugin.

## Required acceptance scenarios

- Exported dimensioned constants use shared variables and exact provider resolution;
  import/export captures values/provenance and updates cannot alter existing
  expressions. The concrete draft includes a sixth contribution schema.
- Kernels validate timestep/configuration/discretization/profile. Invalid timestep
  preserves authored intent and run time; recommendations are not replacements.
- Birth/death history policy is checkpoint-consistent with emitter state;
  spawn/restore/next-step matches uninterrupted execution and missing required
  history is diagnosed, not silently zero-filled.
- Sampling quality survives UI/MCP/recording separately from precision/validity;
  flat bounded buffers have checked ranges/strides and lease-safe cache reuse.
  Include allocation evidence against per-cell heap growth.
- Field painting is not initial work. Reconsider post-MVP only if needed; no
  brush or state-writing sample interface is implied.

- Enforce the accepted one-field/force-stage integrator profile with declared
  bounded scientific history. Reject kernels requiring unsupported stages;
  checkpoint/restore reproduces required history independently of trajectory
  display settings. Expanded hooks/evaluations follow the
  [X-COMPOSITION evidence-triggered review](kagami/define-composed-object-execution.md#follow-up--staged-integrator-capabilities),
  not an unresolved first-pass design gate.

- Integrator selection pins a plugin kernel rather than selecting runtime-owned
  equations. Declare/validate required force stages and carried history; do not
  claim arbitrary RK4/Verlet support from a single force sample. Compatible
  integrator fixtures and their numerical evidence belong to X-COMPOSITION/O-WASM.
- Local install/update never performs registry discovery or automatic downloads;
  untrusted local bundles receive the same validation and sandbox restrictions.

- Named observable channels declare scientific identity, shape, dimensions/SI
  units and coordinate semantics. Include model-dependent electromagnetic scalar
  and vector potentials and kernel-supplied Jacobians as design fixtures. Support
  scalars, fixed-size vectors and fixed-size matrices. Specify encoding/index
  conventions, precision, size bounds and encoding of explicit kernel channel
  declarations referencing scientific contracts. Test model switching preserves
  missing-channel requests as unavailable, while supported channels remain usable.
  Test matrix shape/count mismatch and size overflow before allocation; do not
  assume every kernel exposes every channel.

- Observer-generated positions use the same bounded batched sampling ABI
  regardless of originating shape. Test request limits, stable result
  correspondence and snapshot identity across batches. Kagami point generation,
  density/count expansion and surface/volume layouts belong to K8/K-OBSERVATION;
  implementing those shapes is not an X-PLUGIN specification gate.

- Field initialization/reinitialization uses Kagami's local runtime without a
  cluster connection even when execution targets the cluster. Local and worker
  execution share the engine; the cluster proxy neither steps scientific state
  nor coordinates peers. Returned state passes through the document authority.

- A kernel with opaque non-grid state supplies typed bounded samples usable by
  generic probes/visualization/MCP. Validate channels, units, sample bounds,
  validity and exact state identity; sampling failure cannot alter scientific
  state or block commit. Recordings distinguish opaque-state-plus-sampler from
  sample-only replay capabilities.
- Host-managed opaque transfer rejects cross-instance/schema/boundary/partition
  payloads even when the kernel artifact matches. Buffer reuse preserves live
  snapshot lifetimes and immutable inputs. Do not require zero-copy or assume
  opaque state permits copying native pointers between workers.

- Manual inspector/MCP field reinitialization and domain/compute-parameter edits
  use the pinned kernel and capture valid field state in one atomic revision.
  Test multi-field failure rollback and exact undo/redo without kernel reexecution;
  presentation changes and loading/export do not trigger reinitialization.

- Field creation before/after entity creation produces the same initial setup
  given identical final authored values. Test a non-zero natural default, no
  coupled-entity input to `init`, separate completed-setup rejection without
  silent repair, and first-update force production from supplied entities.
  Setup data must respect sandbox limits and explicit restart/checkpoint semantics.
- Field creation runs sandboxed default construction through the document
  authority; failure/invalid output leaves no partial field. Save/reopen/export
  preserve captured scientific state without rerunning default construction.
  Runtime-only setup can be rebuilt without changing those values. Plugin
  install/pack/inspect still must not execute kernels.

- Vocabulary A supplies mass/gravity and an optional solver; B supplies the chosen
  solver of the exact same contract, C an alternative. B installs before A and
  becomes available when its dependency resolves; unrelated B contributions work
  meanwhile. Availability is not automatic experiment selection.
- Exporting A's mass plus B's solver excludes A's unused executable artifact and
  all authoring-only assets. Validate and inspect the resulting workload without
  any Kagami plugin installation or catalogs. Classical and GEM are independent
  kernel artifacts even when packaged together. Shared source code does not
  justify a multi-solver artifact or function-level stripping at export.
- Validate each declared kernel against its one versioned execution contract;
  test mismatched/missing contract declarations and incompatible ABI. Define the
  checks concretely with O-WASM rather than claiming binary inspection proves
  scientific correctness or absence of arbitrary internal algorithms.
- Provider collisions never silently overwrite; incompatible exact contracts,
  unknown points, disabled dependencies and cycles have bounded diagnostics.
- Existing pins survive additional eligible providers being installed. Unique
  unbound dependencies resolve and are pinned; ambiguous ones require a choice.
  An unresolved selection cannot be exported/admitted as runnable. Missing cache
  bytes are fetched only by pinned digest, never replaced by another provider.
- CLI/MCP ambiguity returns machine-readable eligible choices without prompting
  or guessing. Retrying with an explicit selection uses the same authority and
  revalidates availability, including when inventory changed since the response.
- Unknown valid opaque payload leaves independent contributions usable; corrupt
  payload/digest or malformed common manifest rejects the release atomically.
- Repacking preserves release identity; changing declared assets changes release
  identity; presentation-only changes do not change scientific contract identity.
  Tool-produced IDs independently verify; no self-hash or authors' manual ID step.
- Side-by-side update leaves existing pins unchanged. Disable/override/default/
  remove have distinct tested effects, including built-ins and all-disabled use.
- Oversized/nested manifests, decompression/path traversal, duplicate declarations,
  hash mismatch and interrupted/concurrent inventory writes cannot publish partial
  state. Test real serialized/IO boundaries, not only constructed valid values.
- Existing persisted/workload fixtures retain their declared version semantics;
  incompatible additions use explicit migration/version gates and regression tests.

Run focused public-interface and malformed-input tests plus repository formatting,
lint, relevant integration/doc tests and `make docs-check`. Record implemented
slices and exact evidence, leaving dependent work gated rather than marking the
whole programme complete.

## Non-goals

Native host plugins; contributed executable UI; Kagami compiler/editor; automatic
model selection by install order; whole-bundle workload export; worker package
management; new distributed scheduling; implementing the gravity/GEM/Maxwell
solvers themselves; and silently expanding the local MVP into a public registry.
