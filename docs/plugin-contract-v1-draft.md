# X-PLUGIN v1 — accepted contract

Status: **Accepted on 2026-09-16; implementation and conformance evidence pending**.  
Review baseline: 2026-09-15. Owner: X-PLUGIN with X-COMPOSITION, X-FIELDS,
O-WASM, S-WORKLOAD and K-DOCUMENT.  
Decision record: [ADR 0027](adr/0027-plugin-contributions-and-immutable-releases.md).  
Implementation tracking: [X-PLUGIN](tasks/define-and-implement-plugin-contract.md).

## 1. How to review this document

The user accepted this concrete MVP contract on 2026-09-16. R1–R8 decisions,
including text originally labelled “proposed”, are now the implementation baseline.
The historical filename is retained to preserve links. “Must” describes required
behavior, not delivered software. Code fragments remain schema notation, not
generated bindings; placeholder digests are not golden vectors. Section 11 tracks
remaining executable specification/conformance work, not another approval of these
design choices. Explicitly deferred features and integration gates remain deferred.

| Review package | Concrete proposal |
| --- | --- |
| R1 | Six extension points including exported constants; exact provider/contract identities and dependency binding |
| R2 | Deterministic CBOR identities; isolated stored-ZIP local packages |
| R3 | Two execution contracts under one common sandbox lifecycle; fixed force/integrate profile |
| R4 | Typed point sampling, validity and snapshot lifetime rules |
| R5 | Captured opaque field state, atomic reinitialization and document persistence |
| R6 | Selected-closure export with bounded release-manifest provenance evidence |
| R7 | Transactional local inventory, command outcomes and concrete resource ceilings |
| R8 | Acyclic crate ownership, compatibility migration and conformance evidence |

Accepted scope: independent plugin contributions; immutable releases; exact
scientific contracts; contribution-level availability; local-only distribution;
no executable visual extensions; kernel-owned opaque field storage; observer-owned
point generation; pluggable integrators; no multi-stage force reevaluation in the
first profile. Registry/discovery/signatures are post-launch. Shape generators
and staged integrator hooks have separate tasks, not hidden gates here.

Accepted Field CAD review refinements: exported dimensioned constants, explicit
kernel timestep validation, entity birth/death history lifecycle, successful-sample
quality metadata and flat bounded sample buffers. Field brush/painting operations
are post-MVP and demand-driven, not initial authoring requirements. Concrete
encodings and lifecycle schedule below are part of the accepted baseline; cross-task
integration still requires the stated compatibility and conformance checks.

## 2. Identities and compatibility (R1)

Proposed textual identifiers are case-sensitive ASCII, never silently normalized:

| Type | Representation and meaning |
| --- | --- |
| `PluginId` | Reverse-DNS-style name; dot-separated lowercase segments matching `[a-z][a-z0-9-]*`, 1–8 segments, total at most 128 bytes |
| `PluginReleaseId` | SHA-256 of canonical release-manifest bytes; external spelling `sha256:` plus 64 lowercase hex digits |
| `ExtensionPointId` | One exact platform-owned identifier from section 3, version included |
| `LocalContributionId` | `[a-z][a-z0-9-]*`, at most 64 bytes, unique throughout a release |
| `ContributionRef` | `{release, extensionPoint, localId}`; not a concatenated string with ambiguous delimiters |
| `ScientificContractRef` | `{name, version: u32 > 0, digest}`; name follows the proposed `PluginId` grammar |
| `ExecutionContractId` | Exact platform-owned identifier from section 5; not a plugin scientific name |

The human release version is a bounded label, not identity or an instruction to
select a release. MVP `update` uses an explicit bundle; it does not sort labels
to discover a “latest” release. Logical names are not authenticated publisher
ownership. Local conflicts are visible provenance choices, not registry claims.

Two same-name/version scientific contracts with different digests are distinct
and incompatible for exact matching. Identical digests under different declared
names/versions are also not the same reference. Scientific references carry the
digest of their canonical scientific declaration (section 4). Labels, icons,
preferred display units and other presentation annotations are excluded; actual
dimensions, coordinate/gauge conventions, constraints and defaults are included.
An executable digest change changes the release/model selection, even if its
scientific interface remains identical.

### Provider resolution

An existing exact pin wins; missing/disabled/incompatible pins become unavailable
without replacement. New dependencies consider contributions from enabled plugins'
default releases and exact compatible providers already selected in that experiment.
Non-default installed releases are selectable explicitly, not automatic candidates.
One eligible provider resolves automatically; more than one produces a bounded
`AmbiguousProvider` outcome with exact candidate references. Zero gives a structured
unavailability reason. The caller explicitly selects and retries against current
inventory/document revisions. Persist the selection. Installation order never wins.

Resolve declared transitive requirements as a bounded graph. A cycle is unavailable
in v1, even if all members are installed; it is not a reason to reject otherwise
valid independent contributions. Candidate ambiguity is not resolved by backtracking
to the first satisfiable provider. A diagnostic lists direct blocking requirements
and bounded transitive causes. Unsupported extension payloads remain opaque and
dormant; their dependents are dormant too. Invalid common metadata or corrupt bytes
reject the release as a whole.

## 3. Release manifest and contribution payloads (R1)

Proposed root resource uses `orishu-resource` structurally:

```text
apiVersion: orishu.plugin/v1
kind: PluginRelease
metadata: { pluginId, versionLabel, displayName?, description? }
spec:
  contributions: Contribution[]
  artifacts: Artifact[]
```

No `status`, release self-ID, installed path, download URL or credential is allowed
inside this root. Source manifests may use paths; pack resolves those to descriptors.
Release-wide unknown keys and duplicate keys are errors. Optional values are omitted,
not null. A contribution is:

```text
Contribution {
  localId, extensionPoint,
  payload: ArtifactRef,
  requirements: Requirement[],
  annotations?: {label?, description?, group?, preferredUnit?, iconArtifact?}
}
Requirement = {slot, contract: ScientificContractRef}
            | {slot, localContribution: LocalContributionId}
Artifact = {digest, sizeBytes, mediaType}
```

Requirements have unique slot names. Unknown extension points may declare only
these understood dependency forms; no undeclared dependency can activate them.
Artifact references resolve by exact digest to a root descriptor. Root descriptors
are unique by digest; contradictory size/media declarations reject the root.
Shared artifact bytes are allowed; independent alternative kernels remain separate
artifacts. All package files are declared artifacts except the root manifest itself.

Initial extension points, with typed versioned payloads:

| Identifier | Required scientific payload |
| --- | --- |
| `orishu.model.components/v1` | Contract name/version; property schemas; role (`data`, `dynamics`, `field-coupling`); role bindings; defaults and constraints |
| `orishu.model.fields/v1` | Contract name/version; domain dimension (3 initially); supported domain/discretization requirements; required observable contract slots |
| `orishu.model.observables/v1` | Contract name/version; physical meaning; scalar/vector/matrix shape; SI dimension; frame/axis/convention declarations |
| `orishu.model.constants/v1` | Contract name/version; named dimensioned scalar values/defaults; scientific meaning and exact dependency references |
| `orishu.compute.field-models/v1` | Field contract slot; coupling component slots; configuration schema; state format ID/version; kernel artifact; execution contract; supported observable slots; profile/limits |
| `orishu.compute.integrators/v1` | Dynamics component slot; configuration and history formats; kernel artifact; execution contract; supported profile; required history bounds |

Vocabulary contributions need no executable. A model's field and coupling providers
may be in another plugin. Explicitly supplied observable channels may exceed a field
family's required minimum; a model must supply that minimum to implement the family.
Probes can preserve additional requests as unavailable after an otherwise valid
model switch. Never invent numeric zeroes for missing support.

Component property types initially reuse the catalog's quantity-expression,
boolean and bounded-text concepts. Vector/matrix *observation* shapes do not silently
extend editable property schemas. Role bindings identify the properties consumed
by standard bulk inputs: Dynamics binds inertial mass; a field coupling declares
source/response quantity properties by name and dimension. They need not be equal.
Entity position/velocity are intrinsic state, not plugin-selected memory layouts.
Configuration parameters use the same property schema and shared expression engine;
execution receives validated finite resolved values, never expression evaluation
inside a step. Defaults and dimension constraints are scientific, not UI metadata.

No free-form executable validation expressions or UI layouts. Cross-field rules
not expressible by this initial schema require a future contract revision. A
kernel's bounded scientific validation operation may diagnose model-specific
initial-state constraints; it must not silently repair authored values.

### Exported constants and variables

The constants contribution supplies bounded named, dimensioned scalar declarations
to the shared variables engine, not a process-global mutable registry. Proposed
payload: `scientific {name, version, constants: [{id, dimension, valueSI, meaning}]}`
with optional presentation metadata outside its scientific hash. These initial
values are literals; derived authored values use the existing expression engine.
Changing a scientific value or dimension changes the contract digest.

Kagami exposes a read-only projection of installed contributions. Import/binding
into an experiment captures exact provider/contract identity and the scientific
values in that revision. Authored aliases are explicit and unique; collisions use
normal provider resolution, never last-registration-wins. An installed update
cannot change existing values or expression results. A researcher can explicitly
copy a default into an editable variable, with provenance; that does not modify
the exported constant. Export captures values and relevant expression/provenance
closure, so workers need no plugin inventory lookup or live authoring namespace.

### Illustrative independent-provider fixture

```text
org.example.gravity-vocabulary:
  mass       -> orishu.model.components/v1
  gravity    -> orishu.model.fields/v1
  strength   -> orishu.model.observables/v1   # vector3, acceleration dimension
org.example.gravity-classical:
  classical  -> orishu.compute.field-models/v1
                requires gravity, mass, strength exact scientific contracts
                artifact classical.wasm
org.example.gravity-gem:
  gem        -> orishu.compute.field-models/v1
                requires gravity and its exact compatible coupling contracts
                artifact gem.wasm
org.example.dynamics:
  dynamics   -> orishu.model.components/v1
  euler      -> orishu.compute.integrators/v1
                artifact euler.wasm
```

Names above are illustrative, not reserved built-in package IDs. Actual schemas
must establish whether particular classical/GEM models meet the same family contract;
the fixture must not infer compatibility from the names alone.

## 4. Canonical identity and local bundle (R2)

Proposal: use the existing workload deterministic-CBOR primitive profile without
changing its implementation semantics: definite lengths, encoded-key byte ordering,
shortest integers, binary64 floats, no tags/null/non-finite values, no trailing bytes.
Use explicit typed projections, not serde layout as the identity definition.
Retain exact string bytes; no Unicode normalization. Set-like collections sort:
contributions by local ID, descriptors by digest bytes, requirements by slot;
ordered semantic arrays (matrix axes, positional schema lists) retain their order.
Reject duplicates before sorting. All emitted defaults are explicit schema values.

Release identity hashes the canonical root above. Artifact identity hashes raw
artifact bytes. A vocabulary payload has two parts: `scientific` and optional
`presentation`. Its contract digest hashes only canonical `scientific`, including
name and version but excluding its own digest and provider release. References
inside declarations use named requirement slots; resolving a slot never inserts a
release self-reference into hashed bytes. All such referenced exact contracts are
included in the scientific requirement declaration. Changing a referenced contract
therefore changes the declaring contract. Reject self-referential contract-digest
cycles. Pack computes local contract digests in dependency order before the root.

Proposed `.okplugin` bundle is a ZIP with **stored entries only** in MVP:

```text
manifest.cbor
blobs/sha256/<64 lowercase hex digits>
```

No directory entries, links, absolute paths, `..`, backslashes, encryption,
duplicate entries, compression, multi-disk or ZIP64 in this profile. The initial
codec also excludes extra fields, comments and streaming data descriptors; require
contiguous non-overlapping local records without a prefix or unaccounted bytes.
Validate local
and central-directory agreement, CRC and exact sizes; SHA-256 remains content
authority. Reject extra/undeclared files and missing declared artifacts. Iterate
bounded entries without extracting attacker-controlled paths. Source pack reads
explicit regular files; it rejects symlinks and path escapes from the source root.
Unknown contribution payloads are digest-verified raw artifacts, never parsed as
untrusted executable instructions. Repacking timestamps/order does not change
release identity. Archive restrictions are distribution policy, not scientific
identity. Compression is a later profile, not an implicit decompressor dependency.

## 5. Runtime and kernel exports (R3)

Orishu owns proposed contracts `orishu:simulation/field@1` and
`orishu:simulation/dynamics@1`. Each kernel implements exactly one. Common setup,
validation, sampling and checkpoint exports do not count as separate scientific
execution contracts. ABI inspection rejects absent/wrong exports and unauthorized
imports; it cannot prove a kernel's equations are correct.

Use capability-scoped bounded resources rather than Rust pointers or one callback
per particle. The following is an interface inventory for WIT generation; exact
WIT/package syntax is an acceptance artifact, not implied executable code here:

Implementation checkpoint: concrete [WIT](../crates/orishu-plugin/wit/simulation.wit)
and generated bindings now exist alongside an isolated field/Dynamics lifecycle host and
separate actual Component fixtures. The [ABI checkpoint](runtime-component-abi.md) records
the mapping and remaining security/admission gates; it does not claim full runtime
or scientific execution. The list below retains the logical contract notation.
Standard role-bound byte projections and deterministic reduction are now specified
and implemented in [scientific bulk IO](scientific-bulk-io.md). Shared instance/
validation envelopes and real Newtonian/Euler adoption now exist; selected workload
closure admission is implemented in the shared runtime. Generic sampling and a
[single-partition atomic owner](runtime-fixed-run.md) now exist, without implying
worker/application integration. Exact context shape alone is not proof of provenance.

Opaque natural-state initialization supports host-owned byte/value ceilings rather
than requiring host knowledge of a kernel's layout. The WIT output grant explicitly
reports whether its descriptor is an exact extent or independent ceilings. Kernels
finish with actual counts; contiguous coverage, ceilings and exactly-once completion
are enforced before candidate extraction. This never weakens exact grants used for
known scientific packet layouts. Returned bytes, not unused capacity, are captured
and hashed. Initialization capacity is policy, not a scientific input: sufficient
different capacities must produce the same state for the same scientific inputs.

```text
common.setup(InstanceContext, ResolvedConfig) -> Session | KernelError
common.load(Session, StateBundle) -> Result
common.validate(Session, ValidationInputs) -> Diagnostics
common.checkpoint(Session, SnapshotRef) -> StateBundle
common.restore(Session, StateBundle) -> Result
common.close(Session) -> Result

field.initialize(Session, DomainDescriptor) -> StateBundle
field.advance(Session, StepContext, priorField, coupledEntities, outputs) -> Result
field.sample(Session, SampleRequest, readonlySnapshot, outputs) -> Result

dynamics.initializeHistory(Session, initialEntities) -> HistoryBundle
dynamics.transitionEntities(Session, BoundaryContext, births, deaths, history, outputs) -> Result
dynamics.integrate(Session, StepContext, entities, forces, history, outputs) -> Result
```

`InstanceContext` binds exact kernel artifact, contribution, state format, runtime
profile and limits. `StepContext` binds workload/run/epoch, committed boundary,
simulation time, positive finite SI `dt`, invocation and partition/coverage IDs.
`DomainDescriptor` carries shared geometric bounds and resolved discretization;
`ResolvedConfig` carries the selected model's declared initial/boundary parameters.
`StateBundle` is versioned
opaque portable blocks plus descriptors; it cannot contain host pointers. Writable
outputs are isolated candidate grants; input grants are immutable and invocation-
scoped. `Session` is runtime-local, never persisted as a process pointer.

`setup` reconstructs disposable runtime data only. Scientifically relevant hidden
history belongs in explicit state/checkpoint blocks. `initialize` constructs natural
defaults without entities; it is not called on file reopen or workload admission to
replace supplied values. Scientific validation can receive the completed experiment's
declared projections read-only; initialization cannot. Results/errors are bounded.
All exports remain sandboxed without ambient filesystem, network, clock or RNG.

### Numerical admissibility and timestep validation

The implemented [R3 input refinement](scientific-bulk-io.md#resolved-configuration-and-domain)
defines shared bounded CBOR configuration values (dimensioned quantities, booleans,
text) and an origin-preserving three-dimensional domain with explicit continuous
or Cartesian-cell discretization. Authoring resolves defaults/expressions once;
worker admission validates captured values without regenerating defaults. Physical
boundary policies/values remain selected-model configuration, not global physics.
These new input-artifact schemas do not silently migrate legacy workload/document
formats; their explicit integration remains required.

`ValidationInputs` must include the proposed SI timestep, domain/discretization,
resolved configuration, selected execution profile and the kernel's declared
relevant initial-state/entity projections. All selected field and Dynamics kernels
validate these inputs before admission/start; compatible ABI and positive finite
`dt` alone do not establish numerical admissibility. Local authoring validation
provides the same feedback, but workers revalidate independently.

Proposed outcome: `Admissible` or `Rejected {code, subjects, message}`, with optional
`TimestepAdvice {upperBoundSeconds?, recommendedSeconds?, conditions}`. A numerical
limit is kernel-owned and conditional on the supplied configuration, not a universal
host formula. An advisory value is not an authorization to rewrite the timestep.
Domain/configuration changes revalidate the authored `dt`; preserve it and report
incompatibility rather than automatically setting a fraction of a solver limit.
Validate before advancing the clock or committing any candidate. Required runtime
checks for evolving-state constraints can reject a step without publishing it.

### Entity membership and numerical history

The history contract must cover births and deaths, not only initial entities.
Proposed initial schedule: births/deaths are admitted as part of boundary `N+1`;
newborns join field-force evaluation at `N+1 -> N+2`, never partway through the
force stage producing their birth. Runtime-provided membership deltas and generated
identities follow ADR 0021; this is not a second emitter scheduler. Final timing
must be reviewed with X-EMITTER before its implementation.

For every birth, the selected integrator explicitly constructs bounded history
from the newborn's initial state under its declared cold-start policy. A required
missing entry is an error, not an implicit zero force. A kernel may intentionally
define a zero-history startup, but must declare and test its numerical consequence.
Deaths retire corresponding active history atomically; retained checkpoints keep
their original history. History binds entity identity, kernel artifact, history
format and boundary; changing the integration kernel cannot reuse it by name.
This profile does not permit live integrator replacement. Authoring replacement
initializes a new run's history explicitly rather than relabelling old history.

Checkpoint all required numerical history with the entity membership and emitter
accumulators, counters and deterministic random-stream state. Failure in history
initialization or membership updates rejects the candidate boundary. The conformance
journey is spawn, checkpoint, restore, next-step equivalence, including empty history,
removed entities and failed birth. Observer trail retention is independent.

### Fixed scientific profile

Proposed profile ID: `orishu.force-then-integrate/v1`. An experiment selects one
integrator for its Dynamics-enabled entity set in this initial profile. Per-entity
integrator groups are deferred. Every active field has one selected model instance.

1. Each field reads its own prior state and matching entities from the same committed
   boundary. It writes candidate field state and a force vector for each declared
   responding dynamic entity, with explicit zero values where the force is zero.
2. Dynamics consumes complete force batches in ascending field-instance ID order,
   grouped by stable entity ID, reduces in that fixed order and integrates once.
   Missing/duplicate/unexpected entries reject the candidate, not imply zero.
   No coupled fields means an explicit empty force set and ordinary inertial motion.
3. Validate all required outputs and commit field/entity/history state atomically.

The force is a single evaluation supplied for this step at committed entity
kinematics. The field model declares its field-state time convention in its
scientific metadata; this profile cannot request additional trial entity states.
The integrator must declare support for this exact convention/profile. This does
not claim fourth-order integration from a frozen force, select an Euler formula,
or assert compatibility of arbitrary Verlet variants. Kernel-private numerical
history is bounded and checkpointed. Graph roles/phase labels do not grant extra
evaluations. Unsupported stage requirements reject admission explicitly.

The initial proof fixture is single-partition local/worker-equivalent execution.
Distributed profiles additionally declare exchange schemas, coverage and legal
partitioning; they cannot treat an arbitrary slice of opaque memory as a valid
halo. Full multi-node implementation is not required for X-PLUGIN's pure contract
slice. Retain the [staged integrator review](tasks/kagami/define-composed-object-execution.md#follow-up--staged-integrator-capabilities).

## 6. Typed sampling (R4)

Sampling uses the selected field kernel, not an independently chosen decoder:

```text
SampleRequest { requestId, snapshotRef, fieldInstanceId, channels[], points[] }
Point { sampleId: u64, positionMetres: [f64; 3] }
Channel { contractRef, shape, dimension, frame, axes, conventions }
Shape = scalar | vector {length} | matrix {rows, columns}
SampleCell = valid {values: f64[], qualityFlags} | invalid {reasonCode}
SampleResponse {requestId, snapshotRef, fieldInstanceId, channelRefs[], cells[]}
```

`SampleCell` is the logical value model only, not a heap allocation or ABI record
per point/channel. Proposed physical response uses one descriptor per channel plus
flat caller/runtime-owned grants: packed numeric values, validity codes and quality
flags. Each channel descriptor declares checked element offsets, point stride,
component count and lengths; matrix components remain row-major. All channels use
the request's point order. Offsets/ranges cannot overlap illegally or escape grants.
Invalid numeric slots are unspecified and must never be consumed; validity governs
them. Reuse allocated output capacity rather than allocating per sample/cell.

Implementation checkpoint: the [scientific bulk IO profile](scientific-bulk-io.md#requested-channel-sampling)
now fixes `OSQ1` request and `OSP1` response framing. Channel descriptors are derived
from the exact identified request into checked flat ranges rather than duplicated
inside the response. The response binds the entire request digest; recordings must
retain request/context alongside response bytes. Pure validation and the actual
Newtonian Component exercise this profile. The fixed owner now provides bounded
detached committed-field leases; observation retention and UI/MCP adoption remain
integration work, not completed R4 delivery.

Successful values also report sampling quality independently of compute precision.
Proposed flags describe direct evaluation, interpolation and reconstruction, with a
channel-level default and per-sample overrides where necessary. Combined operations
may carry multiple flags. “Direct” is not a claim of mathematical exactness. Quality
definitions and interpolation/reconstruction conventions are part of the declared
channel/model contract; unsupported or unknown flags cannot silently mean exact.
UI/MCP and recorded observations retain these distinctions alongside f32/f64 compute
provenance. Invalidity is not encoded as a quality flag.

Cache/reuse is keyed by snapshot identity, field/kernel/configuration, exact channels
and positions plus relevant query conventions—not only a probe geometry ID. A leased
buffer cannot be overwritten until all consumers release it. Bounded cache misses or
eviction must not change scientific values, validity, quality or step commitment.

Proposal: finite binary64 on this interchange boundary; kernels may compute with
another declared precision, which observation provenance must report. No claim
of extra accuracy follows conversion to f64. Each scalar/vector/matrix channel
has one physical dimension in SI base-dimension order, using the shared unit
engine's dimension representation. Matrix encoding is row-major. For a spatial
Jacobian, `J[i,j] = d(component i)/d(world coordinate j)`; channels explicitly
declare axis meaning and conventions, including gauge/reference for potentials.
Vector length or matrix shape alone never implies spatial meaning.

Logical cells are point-major then request-channel order; the physical flat
channel buffers above preserve that correspondence. Sample IDs are unique within
a request; routing/chunking preserves correspondence. Invalid cells carry no
numeric payload. Per-cell reasons: `OutsideDomain`, `Undefined`, `Singular`,
`ChannelUnavailable`. Invalid IDs/shape, oversized requests and stale snapshots
are whole-request errors. Kernel trap/exhaustion gives a failed query, not valid
zeroes or partially advertised complete data. Incomplete delivery explicitly
reports missing coverage; it never mixes snapshot identities.

Positions and channel counts are bounded before multiplication/allocation. All
cells refer to one pinned immutable authored or committed snapshot. A query lease
pins that state only within the observer budget. If retention is unavailable,
reject/cancel the query rather than delaying a scientific commit. Sampling uses
an isolated guest context or equivalent enforceable snapshot isolation; read-only
function naming alone is not protection against guest mutation. Candidate state
cannot be sampled. Full-domain views use explicit finite sampling coverage, not
an implication that sampled values are a restorable checkpoint.

Kagami generates point/plane/sphere/box/cylinder batches, including surface/volume
modes. Geometry algorithms and density controls are K8/K-OBSERVATION work, not
kernel APIs. Flow-line algorithms may request Jacobian channels through the same
operation. Archived opaque state needs the exact kernel/required artifacts for
new retrospective queries; sample-only recordings expose only recorded coverage.

## 7. Authoring and captured field state (R5)

Field painting/brushes are explicitly excluded from MVP. If user demand warrants
them post-MVP, specify bounded kernel-owned state-edit intent through the document
authority, returning candidate state atomically. Do not add writes to sampling or
expose private field layout as an editing API. The PoC brush experiment is historical
evidence, not a required capability to port.

The local runtime embeds the worker engine; a cluster proxy implements the public
runtime interface without peer coordination or independent stepping. Kagami keeps
local initialization available even with a remote execution target. Neither runtime
becomes document authority. Proposed public operations are initialize/reinitialize,
validate, admit/start/stop and acquire/sample/release snapshot, returning typed
outcomes. Cluster proxy run operations use O-CLIENT; local authoring initialization
does not require a formation or silently route to a remote cluster.

Creating a field invokes the pinned kernel locally, validates its default output,
and atomically stores configuration, provider pins and captured state. No coupled
entities enter initialization. Domain/compute-parameter changes and manual inspector/
MCP reinitialization do the same. A shared-domain edit regenerates all affected
fields or accepts none. Presentation-only changes do not regenerate state.

Use an effect token containing document identity, expected revision and proposed
operation to guard asynchronous initialization. Results from replaced/edited
documents are rejected as stale. The old revision remains readable until acceptance;
failed validation, resource exhaustion or cancellation leaves it unchanged.
Undo/redo retain before/after state descriptors and their blobs, not a recipe that
reruns kernels. Save pins a coherent revision and all referenced blobs while writing.

Implemented self-contained [document-container v4](experiment-container-v4.md): stored ZIP containing
`document.json` and digest-addressed `blobs/sha256/...`, with the same path safety
rules as plugin bundles. The versioned document references opaque state by size,
digest, kernel artifact, model/field contract and state-format identity. Do not
embed unbounded binary arrays in the existing JSON format or rely on mutable
external sidecars. The version audit reserves v4 for scientific containers and
preserves JSON v1–v3 readers/writing semantics. Exact release evidence and selected
declarations are embedded, but executable code is not required to reopen a draft.
Workload export still requires independent full selected-code verification.
Existing files remain readable through the existing reader and explicit import.

Default states must satisfy structural bounds; completed scientific validation
may diagnose later source/configuration incompatibility without rewriting intent.
Drafts may retain unavailable contributions and observations. Export refuses missing
scientific dependencies. Unavailable observer-only requests remain visible and are
omitted from active sampling with an explicit report, not silently dropped or used
to block scientific stepping. Mandatory retained result requirements, if introduced
later, need a distinct reliable-result contract rather than reusing ideal probes.

## 8. Selected workload closure and provenance (R6)

Compilation freezes resolved quantities, exact contribution bindings, field state,
integrator/history definitions, graph/profile, selected code and runtime inputs.
No catalog path, installation preference or mutable tag selects executable physics.

Retain workload v2 bytes unchanged. The [v3 extension](workload-v3.md) adds
an identity-bearing plugin-selection descriptor referencing:

```text
Selection {
  roots: ContributionRef[],
  contributions: ContributionRef[],
  bindings: {consumer, requirementSlot, provider, exactContract}[],
  kernelInstances: {instanceId, contribution, executionContract}[],
  releaseEvidence: ArtifactRef[]
}
```

The evidence descriptor and independent selected-byte compiler/verifier are now
implemented in `orishu_plugin::selected`; see the
[wire/validation notes and integration limits](plugin-selected-closure.md).
`roots` records explicit usage before dependency expansion so extra unreachable
contributions can be rejected. The v3 root/captured-definition codecs and exact
context checks are implemented, as are fixed scientific-profile assembly and
independent captured-state admission. Scene-bearing execution v2 now binds shared
entity composition and source evidence to the numerical packets; see its
[schema and compatibility rules](workload-v3.md#scene-bearing-execution-v2).
Unix headless `kagami export` now produces a [portable selected closure](workload-bundle-v1.md)
from saved captured state and exact installed code, without running initialization.
Emitter/dynamic-membership support, window export and worker endpoint
adoption remain integration work; workload-v2 and numeric-only descriptor bytes
are unchanged.

Each evidence artifact contains the complete canonical root manifest of a selected
release. Recompute its release digest and verify each selected contribution's
payload membership. Include selected payloads and their transitive scientific
dependencies; do not fetch unselected artifacts listed by evidence. Such descriptor
lists are **historical membership evidence**, not executable workload dependency
edges. Only selected executable/input references enter the required closure.
Unknown unselected payloads are not parsed or activated at admission.

This deliberately carries a bounded amount of unused *manifest metadata*, not
unused code/docs/icons. It avoids inventing a Merkle-proof format initially.
Any package metadata included in root evidence is visible to workload recipients;
authors must never put secrets there. It proves membership in a content-addressed
release, not publisher authenticity. No signature trust claim is made.

Admission independently checks root/evidence digests, all required selected payloads,
exact bindings, artifacts, ABI/profile, dimensions, topology, ownership, limits and
initial-state validation. A root with a valid checksum is not sufficient. Missing
artifact bytes may be fetched by exact digest through the workload artifact-delivery
contract; unresolved
provider choices may not. Identical bytes cached under different locations remain
identical input. Unsupported workload profiles fail closed in older workers.

Plugin installation/enablement remains Kagami authoring state, not worker state.
The submitter supplies the complete selected closure; workers receive missing
digest-addressed bytes through upload/peer transfer and verify them before use.
Verified resident bytes can be reused. This is the intended delivery boundary,
not a claim that worker upload/admission/transfer APIs are already implemented.

[O-ARTIFACT-ADMIN](tasks/implement-artifact-cache-administration.md) records the
operator follow-up: inspect residency, pre-position bytes, manage retention and
eviction, and deny compromised required digests. Cache presence never grants
execution permission. Cache eviction, stored-resource purge and execution denial
are separate controls; policy convergence, lifetime and active-run containment
remain explicit security design gates. This does not expand local-only plugin
distribution MVP into a cluster plugin manager or registry.

## 9. Local inventory and CLI (R7)

Proposed inventory stores immutable content-addressed blobs and release manifests,
plus a versioned index of installed releases, defaults, logical enablement and local
origin metadata. Source paths belong only to origin metadata, never release identity.
One process-wide/cross-process lock serializes mutations; readers consume an immutable
index revision. Stage writes, verify, flush, atomically replace the index and flush
its parent. Crash recovery ignores unreferenced staging; readers see old or new
index, never partial installation. Require same-filesystem atomic replacement.

Kernel invocations and open documents lease required blobs. Removal de-registers
a release but cannot delete leased blobs under active readers; physical reclamation
is deferred until local leases permit it. Explicit removal warns about known open
documents, not every file on disk. No automatic GC based on incomplete document
discovery. Disable affects future availability without interrupting accepted runs
or rewriting drafts; in-flight authoring effects revalidate before acceptance.

Proposed command grammar (all support bounded `--json` outcomes):

```text
kagami plugin validate <source-directory-or-bundle>
kagami plugin pack <source-directory> --output <bundle>
kagami plugin inspect <bundle-or-release-id>
kagami plugin install <bundle> [--expect-release <digest>]
kagami plugin update <plugin-id> <bundle> [--expect-release <digest>]
kagami plugin list [--all-releases]
kagami plugin set-default <plugin-id> <release-id>
kagami plugin enable <plugin-id>
kagami plugin disable <plugin-id>
kagami plugin remove <plugin-id> <release-id> [--ack-open-references]
kagami --enable-plugin <plugin-id> --disable-plugin <other-plugin-id>
```

First install of a logical plugin enables it and sets its default. Additional
`install` preserves the current default; `update` validates matching logical ID,
installs the explicit bundle and selects it as default. Neither migrates documents.
Removing the default with other releases installed requires a prior explicit
set-default; removing the final release removes its default. Conflicting startup
overrides for the same ID are errors, not order-dependent flags. Built-ins obey
the same enablement rules. Validate/pack/inspect never execute guest code; scientific
default construction and validation are separate runtime operations.

Structured outcomes contain `code`, bounded message, subject/path, optional exact
references, expected/actual revision and bounded candidate/causal details. Initial
codes include `Malformed`, `LimitExceeded`, `IntegrityMismatch`, `UnsupportedVersion`,
`UnavailableDependency`, `AmbiguousProvider`, `StaleRevision`, `InvalidSelection`,
`KernelFailure`, `InvalidScientificState`, `InUse`, `IoFailure`. Success of transport
does not mean command acceptance. Full candidates may be paginated against one
inventory revision; truncation is explicit. Tokens/credentials never enter outcomes.

### Proposed default ceilings (review, not measured capacity claims)

These are caller-owned limits, checked before allocation/work. Consumers may choose
stricter budgets and report them; stored identities do not depend on local budgets.

| Input/work | Proposed ceiling |
| --- | --- |
| Root manifest / one known contribution payload | 1 MiB / 256 KiB |
| Contributions / artifact descriptors per release | 256 / 4096 |
| Total local bundle / individual artifact | 1 GiB / 256 MiB |
| Structured nesting / dependency depth / resolved nodes | 32 / 32 / 4096 |
| Required slots per contribution | 64 |
| Channels per kernel / channels per sample request | 128 / 16 |
| Points per sample batch / matrix dimensions | 4096 / each 1–16, at most 256 elements |
| Sample response / runtime transfer chunk | 8 MiB / 1 MiB |
| Diagnostic count / message bytes / returned candidates per page | 64 / 512 / 32 |
| Initial local guest linear memory / exported initial-state bytes | 256 MiB / 128 MiB |
| Document container aggregate bytes | 512 MiB |

Products of limits must also pass checked arithmetic and response-byte limits;
individually valid maxima need not be valid together. Define per-export instruction
fuel and interrupt deadlines in the O-WASM host profile using actual engine units
and fixtures before executing untrusted kernels. This draft deliberately does not
invent a portable instruction-cost constant. Observation scheduling must reserve
step resources independently and cancel observers on budget exhaustion.

## 10. Crate ownership and migration (R8)

Source inspected for this draft, not merely task status:

- `orishu-workload` owns `orishu.dev/v2`, artifact/digest types, canonical CBOR and
  `ComponentInstance` with `roles`, logical plugin/model/schema IDs and scalar config.
- `kagami-catalog::SchemaRegistry` keys by `ComponentTypeId` and replaces on insert;
  its `PropertyKind` is quantity/boolean/text, not release-aware plugin schemas.
- `kagami-document::PluginComposition` stores logical catalog plugin IDs, not pins.
- As of 2026-09-16, `orishu-plugin` implements the declaration/identity slice.
  See its [schema details and acceptance layers](../crates/orishu-plugin/README.md)
  and [fixed fixtures](../crates/orishu-plugin/tests/fixtures/contract-v1.json).
  Pure provider resolution and a bounded stored-ZIP byte codec are also implemented,
  with focused tests; they do not install packages or admit executable code.
  Kagami now has initial Unix source/package IO and durable inventory/CLI
  management; [tooling documentation](plugin-authoring-tools.md) records exact
  source/index formats and limitations. Authoring/UI/MCP and runtime integration,
  source-local symbolic compilation and full crash-injection evidence remain.

Proposed dependency ownership:

| Owner | Contract responsibility and allowed direction |
| --- | --- |
| `orishu-resource` | Existing structural envelope only |
| `orishu-variables` | Existing dimensions, expressions and bounded evaluation |
| `orishu-workload` | Existing artifact/digest/canonical primitives, graph and versioned workload closure; no dependency on plugin/catalog/runtime |
| `orishu-plugin` (new) | Release/contribution/scientific identities, payloads, resolution, selection evidence and pure scientific-policy validation; depends on workload/resource/variables |
| `kagami-catalog` | Template authority/materialization; consumes shared plugin schema types after migration, never duplicated scientific validators |
| Document/session crates | Exact authored pins, state references, commands, revisions and file authority; consume shared contracts |
| Local runtime / worker | Combine structural workload validation with plugin-policy validation and O-WASM execution; no plugin installer in workers |
| Kagami shell | Local package IO/inventory, CLI/UI/MCP adapters, runtime selection; no second scientific engine |

The workload manifest references a versioned selection artifact using workload-owned
descriptors; it does not import plugin types back into the workload crate. Runtime
and compilation call the pure plugin validator explicitly. A “structurally valid”
workload never means scientifically admitted. Share canonical primitives only via
reviewed public helpers; do not copy the codec or make `orishu-plugin` depend on
`crates/orishu`, which pulls networking/async dependencies.

Keep current type/wire names and golden fixtures unchanged until explicit migration.
New release pins cannot be inferred from old logical IDs. Import old documents as
preserved unresolved authoring intent; use normal unique/ambiguous provider resolution
and obtain explicit migrations before executing. Never reinterpret existing field
bytes under a different kernel/state format. Where migration is unavailable, offer
the accepted explicit reinitialization workflow and explain lost authored state.
No executable migration plugin contract in v1. New container/workload versions and
catalog schema adoption need their own decoder fixtures and K4/S-WORKLOAD review.

## 11. Evidence required before specification closure

Field CAD review evidence (historical source references, not interoperable APIs):

- `../field-cad/crates/fieldcad-plugin-api/src/lib.rs`: exported variable types
  around line 91 and registration around 348; timestep validation/advice around
  395–409; sampling buffers around 210 and exactness around 290; buffer reuse
  around 572–602; brush intent around 183/430–437.
- `../field-cad/crates/fieldcad-simulation/src/runtime.rs`: timestep validation
  around 2041, automatic timestep replacement around 2112 (not adopted), Verlet
  half-drift/history around 2282–2303, and spawn batching around 2380.
- `../field-cad/crates/fieldcad-simulation/src/lib.rs`: timestep rejection test
  around 165 and cold-start history test around 704; `src/emitters.rs` around
  247 retains rate/random state. Line references are review-time navigation aids.

These findings are source-review evidence only; this documentation update did not
execute the predecessor tests or claim those behaviors are implemented here.

R1–R8 are accepted. Provide the following checked companion artifacts without
silently changing the accepted semantics:

1. Machine-readable root and six payload schemas, selection/lock schema and full
   gravity/Euler plus alternative-provider fixture, including exact real hashes.
2. Golden canonical bytes/digests for release and scientific projections: reordered
   source maps, changed annotations, changed semantics, duplicate keys, unknown
   payload and archive repack cases. Independent decoder round-trip verification.
3. Complete WIT worlds/generated binding fixture for field and dynamics contracts;
   explicit numeric record layouts, failure enums, fuel limits and handle lifetimes.
4. Pure resolver tests: dormant-before-vocabulary, collisions, cycles, disabled pins,
   explicit migration, missing optional observables and stale ambiguity retries.
5. Workload selection evidence fixture proving A's vocabulary plus B's solver excludes
   A's unused code, while membership evidence validates without fetching that code.
6. Reviewed document/workload version identifiers, ownership/dependency tests and
   hostile-input cases for parsers, storage, initialization and sampling boundaries.
7. Bounded implementation slices marked ready only for reviewed portions. Retain
   explicit gates for engine integration, numerical evidence and distributed profiles.

Additional accepted refinement evidence: constant import/update isolation and
dimension checks; a kernel-defined timestep rejection preserving authored `dt` and
run time; newborn/death history plus emitter checkpoint equivalence; flat-buffer
range/shape tests and allocation evidence; successful sample quality preserved in
UI/MCP/recordings; cache lease/reuse under concurrent snapshot consumers. Use actual
PoC Verlet only in the staged-profile follow-up: its pre-force half-drift does not
conform to this profile's committed-kinematics force input.

Design acceptance does not claim those companion artifacts or implementation tests
already exist. Produce them within their owning implementation slices before
claiming executable specification closure or integrated runtime readiness.
