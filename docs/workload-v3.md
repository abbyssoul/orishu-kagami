# Composed workload v3

Status: **shared format, fixed scientific-profile compiler/verifier and runtime
admission implemented; Kagami/worker application integration remains open**. This is the explicit
version extension approved by [X-PLUGIN R6](plugin-contract-v1-draft.md#8-selected-workload-closure-and-provenance-r6).
It does not rename, reinterpret or rewrite any workload-v2 bytes.

## Why a new root version

Workload v2 requires one positive uniform spatial step, a dimension/side-length
domain without an origin, and an optional global integration selector. That is
not a faithful representation of the accepted analytic/plugin-owned fields,
explicit domain origins, per-model discretization and selected integrator kernels.
Supplying artificial v2 domain values would make unused scientific claims part of
identity. Changing the v2 projection would invalidate existing workload digests.

`orishu_workload::v3` therefore defines `apiVersion: orishu.dev/v3`, `kind: Workload`.
It retains the existing immutable `WorkloadMeta`, `ComputeSpec` component graph,
typed ownership/channels/step plan and `WorkloadRequirements`. Metadata still has
no server-assigned identity or mutable status. It replaces v2's domain/input layout
with explicit selected and captured scientific descriptors:

```text
spec:
  compute: <existing component graph and step plan>
  selection: <ArtifactDescriptor, role plugin-selection>
  execution: <ArtifactDescriptor, role scientific-execution>
  artifacts: <remaining required ArtifactDescriptors>
  requirements: <hardware and numerical requirements>
```

The two mandatory descriptor roles are singular across this root. `artifacts`
contains required evidence, selected payloads, configuration, domain and initial
state blobs; code is already referenced by the component instances. All references
are digest/size/media/schema descriptors without locations or credentials. The
complete direct descriptor list lets byte-delivery/cache adapters operate without
parsing plugin-specific scientific content. Scientific admission must subsequently
prove that the list is exactly the required selected execution closure: a correct
hash of an unrelated extra blob is not sufficient.

## Captured scientific definition

The execution artifact is owned by `orishu_plugin::execution::ExecutionDefinition`.
Its original numeric-only schema is `orishu.simulation.execution/v1`:

- Fixed profile and positive SI timestep.
- Exact complete numeric object-state input, including static/kinematic objects.
- Field uses in canonical instance-ID order, each with a captured context,
  portable state and coupled-object projection.
- One dynamics integrator with its captured context and portable history.

Every input identity includes schema, logical count, byte length and digest.
Context input references name `orishu.simulation.instance/v1`; those contexts
contain captured configuration/domain identities. Initial field and history state
are supplied bytes, never a request to rerun initialization on admission.

The cold definition has a 1 MiB byte ceiling and bounded fields/tree/text; contexts
have their existing 64 KiB ceiling. Its reader checks structure/order/schema tags,
not scientific byte contents or selected-provider agreement. In particular it does
not prove membership, field-family uniqueness, coupling/object consistency or
kernel numerical admissibility. The scene-bearing extension below now preserves
authored component data and expression provenance. Emitter blueprints and dynamic
membership still require explicit profile integration; these cannot be silently
discarded by an exporter claiming full support. The currently implemented run
owner remains fixed-membership/fixed-extent.

`selected::verify_context` separately verifies a loaded context against the exact
selected kernel use, artifact and scientific contract, state format/profile,
field state bound, source/response property identities and dimensions, and complete
observable vocabulary. Negotiated sampling quality and computational precision
remain inputs for kernel admissibility checks. Release metadata used for provenance
is exposed only from independently verified root evidence.

### Scene-bearing execution v2

`orishu.simulation.execution/v2` requires one additional `scene: InputIdentity`
with schema `orishu.simulation.scene/v1` and logical count one. V1 forbids that
field; v2 without it is refused. Existing numeric-only v1 and workload-v2/v3
canonical bytes are unchanged. New scene-bearing workloads have new digests,
and readers without execution-v2 support must refuse them rather than drop data.
The graph/field/Dynamics WIT contracts do not change.

The canonical scene contains:

- Complete objects in stable ID order: label, initial kinematics, orientation,
  angular velocity, optional sphere/box extent and exact attached components.
- Each component's exact selected contribution and complete explicit properties
  (finite dimensioned SI quantities, booleans or text). Quantity source expressions
  and separately authored unit annotations are retained beside resolved values.
- Variable names, original IDs, expressions and descriptions as historical
  authoring evidence. Kernels retain explicitly authored configuration property
  IDs and their quantity sources; resolved literal/default values stay in the
  already captured configuration artifact.
- Optional source-template schema and canonical SHA-256 fingerprint. Catalog
  names, paths and installation locations are not exported. The fingerprint is
  historical evidence, **not** an artifact-fetch edge.

Source expressions are non-executable provenance. Admission validates the actual
resolved values and their dimensions/constraints, not their truth as a claim about
historical authorship; it never evaluates source or applies defaults. All evidence
is identity-bearing. This is a shared runtime scene schema, not a serialized
Kagami document, catalog or editable authority.

Admission proves exact object coverage, component membership and complete required
properties, numerical kinematics/Dynamics agreement, and complete field-coupling
coverage/values. Omitting a coupled record is a refusal, even if remaining records
are individually valid. Only the selected compatible Dynamics provider may drive
objects; a conflicting Dynamics attachment cannot silently become static. Declared
additive data and inactive coupling components survive without becoming executable
physics. Selected closure reachability starts from actual scene components plus
kernel uses, so unrelated selected roots still fail. `VerifiedWorkload::scene`
exposes only independently checked composition.

The current profile integrates translations using point field projections. Geometry
is retained, but it does not implicitly introduce finite-body/collision semantics;
nonzero angular velocity on dynamic objects is explicitly unsupported and refused.
There are no implicit emitter definitions or schedules in scene v1.

Default scene bounds: 16 MiB encoded bytes, 100,000 objects, 400,000 attached
components, one million property/source entries, 16,384 variable records, 65 kernel
source records, 16 KiB per text, two million canonical values, and 16 million
object/field-slot comparisons. Conservative raw-tree/copy reservations, fixed
24-level nesting, overall captured-input budgets and stricter caller policies also
apply. These are cold allocation/work bounds, not total process RSS accounting.

## Implemented checks and admission

V3 reuses v2's graph, descriptor and streaming hash validators, without fabricating
a v2 domain. Structural errors, conflicting roles/descriptors, byte budgets and
aggregate overflow are rejected before any blob-source callback. Canonical reading
bounds bytes/tree depth/value count and typed collections. The hard v3 nesting
ceiling is 64. Raw compiler-built models have collection/text preflight before
canonical tree copies or graph indexing. Field order is preserved; identity is an explicit CBOR projection,
not serde layout. The old v2 projection and its golden vectors remain unchanged.

Tests pin the new fixture digest, exercise canonical/JSON round trips, truncated
prefixes, cross-version rejection, graph/role/budget failures before retrieval,
missing/corrupt bytes and overflow even with a `u64::MAX` byte policy. JSON serde
round trips test the raw model; v3 does **not** yet have the v2 bounded JSON/YAML
authoring adapter. Network/file consumers must use its bounded canonical reader,
not an unbounded serde convenience function.

Generic structural closure does not recognize or authorize execution profiles.
`orishu_plugin::workload::{compile, verify}` implements the explicit
`orishu.force-then-integrate/v1` profile. Compilation generates a root and then
uses the same independent verifier as admission. It retains only selected source
references and generates small descriptors/evidence, not copies of installed code.

The verifier checks exact instance/use coverage, required selected scientific
closure, one model per exact field-family contract, complete input bytes, matching
object/coupling kinematics and Dynamics membership, coupling roles and scalar
property constraints. Resolved configuration must satisfy selected declarations;
all kernels agree on geometric domain bounds while retaining their own supported
discretization. Contexts and state-format identities are checked against exact
selected providers. Defaults are never reevaluated. Unused selected vocabulary,
extra root artifacts, unknown profiles and changes to the derived graph fail closed.

The canonical graph has independent field invocations reading committed objects
and their own field state, followed by one integrator depending on every field.
Fields own `field-N` channels in canonical instance order; Dynamics owns `objects`
and `history`. A shared `forces` sum channel exists only when fields exist. No-field
integration receives explicit zero-force records from the runtime. Components and
channels sort by ID; invocation order is fields then Dynamics. There are no placement
constraints or graph-level configuration overrides in this first profile.

Default profile policy bounds 64 fields, one million objects, four million coupling
records, 128 MiB per captured input and 512 MiB across input uses (including repeated
uses). Root, selection and declaration limits apply independently. Reachability is
indexed by consumer rather than rescanning every edge per contribution. Coupled
projection validation costs O(couplings × log(objects)); property lookup is hoisted
per slot. These cold checks are not a whole-process resident-memory bound.

`orishu_runtime::admit` decodes canonical v3 bytes, applies a caller-supplied digest
deny set to every required resource and the root, independently verifies the
scientific profile, compiles actual selected Components and loads/validates captured
state into `FixedRun` without initialization. Kernel count and aggregate code bytes
are checked before compilation. Unsupported additional hardware/execution-profile
requirements fail closed. The embedding authority supplies the fenced run scope;
this function does not allocate cluster run IDs or enforce a cluster-wide singleton.
JIT cancellation/resident-memory accounting remain hardening work.

Runtime admission tests construct independently verified vocabulary/solver releases,
export only required bytes, then run actual Newtonian/Euler Components in a fresh
runtime with no plugin inventory. They cover no-field drift, missing/extra bytes,
graph changes, denied code, captured-state validation and scientific-input budgets.
Kagami now also has a captured-scene bridge: `kagami_document::projection`
projects a captured document revision using retained exact role declarations and
authored quantities, including offline-reopened unresolved properties. The app's
`workload::compile_captured` requires the prepared installed selection to equal the
document's pins and produces the exact code/input closure through this shared
compiler. Captured field/history bytes are shared unchanged; no initializer runs.
It refuses legacy setup, missing integrators, unsupported components/properties,
schema mismatches and projection budgets instead of dropping scientific intent.
Tests reopen a v4 document, project dynamic and static gravity-coupled objects,
then admit and advance the result through the real runtime without an inventory.

It now serializes shared entity composition and location-free source evidence in
the execution-v2 scene artifact, including independent additive data contributions.
The Unix `kagami export` command now writes a new self-contained
[portable workload bundle](workload-bundle-v1.md) from a saved captured experiment
and its exact installed code. Export never reinitializes state or modifies the
source. The actual CLI file is independently admitted and advanced in tests without
an inventory. Emitter definitions/dynamic membership, window export controls,
submission, protocol negotiation, artifact transfer and worker endpoint adoption
remain required; the worker APIs do not yet accept v3 workloads.

V2 and v3 are intentionally different Rust types within the same shared workload
crate. Neither is a separate product authority. Converting older authoring intent
requires an explicit compiler/migration step with scientific validation; changing
the `apiVersion` string is not a migration.
