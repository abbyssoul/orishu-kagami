# Standard scientific bulk IO — version 1

Status: **implemented packets, instance/validation envelopes and fixed-profile force
reduction, adopted by real reference Components and the single-partition
[atomic run owner](runtime-fixed-run.md); selected workload admission remains work**.
This refines the standard role-bound
projections in the [accepted plugin contract](plugin-contract-v1-draft.md) and
[composition decision](adr/0020-compose-object-behaviour-through-plugin-components.md).
The pure `orishu_plugin::execution` module is shared by host and external kernel
toolchains. No engine, IO, native memory layout or model equation enters that module.

## Meaning and identity

`EntityId` is an unsigned 64-bit identity within an admitted workload/run entity
set. Reading a number establishes no existence or creation authority. The compiler
must preserve authored object identity; the runtime's deterministic birth allocator
and checkpointed membership own new run identities. This packet format does not
allocate IDs, reuse retired IDs or define a second emitter scheduler.

Dynamics input/output projects identity, intrinsic position and velocity, and
strictly positive inertial mass. Positions are metres, velocities metres/second,
mass kilograms, and forces newtons in the admitted domain frame. Raw structs and
serde are not acceptance APIs; packet writers/readers enforce the invariants.

A field's coupled projection contains one record per entity **and coupling slot**.
Its canonical slot table is ordered by the selected field model's coupling
requirement-slot name and binds each index to an exact component scientific
contract, role-property names and SI dimensions. An index outside that admitted
table is an error. Multiple slots on an entity stay separate: the host must not
silently sum source strengths or equate gravitational mass, inertial mass and charge.
The table now belongs to the shared `InstanceContext` below. Checking it against
the complete selected workload/declaration closure remains admission work; a table
supplied to a packet reader is not proof of that relationship.

Source and response are independent optional resolved SI scalars. At least one
must be present per coupled record. An absent value differs from present zero.
`has_dynamics` is derived from the admitted component set, not a new editable
motion-authority flag. Static/kinematic entities can source fields but are not
integrated. An entity requires one force row from a field if **any** of its slots
has Dynamics and a response, including zero response. A kernel owns how its slots
combine into that field's force; the host does not multiply that force by slot count.

All packets require their host descriptor and context: workload/run/epoch,
committed boundary, domain frame, selected kernel/field, slot table and contracts.
The bytes alone do not prove that binding. `Buffer::scientific_batch` checks schema
and logical record count against the packet; the run supervisor must check the rest.
Opaque field state and integrator history remain plugin-owned formats, not these
standard entity/force records.

## Exact bytes

All integers and IEEE-754 binary64 values are little-endian. No native padding,
alignment, pointer, compression or host endian convention is implied. Non-finite
numbers and negative-zero bit patterns are rejected; writers emit positive zero.

Every packet starts with this 12-byte header:

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | ASCII `OSB1` (this version) |
| 4 | 2 | Record kind below, unsigned integer |
| 6 | 2 | Reserved, must be zero |
| 8 | 4 | Unsigned record count |

The remaining bytes are exactly `count × record size`, without trailing bytes.
An empty packet still contains its header; missing data is not an empty packet.

| Kind | Descriptor schema | Record bytes | Record layout |
| --- | --- | --- | --- |
| 1 | `orishu.dynamic-entities/v1` | 64 | ID `u64`; position `3×f64`; velocity `3×f64`; inertial mass `f64` |
| 2 | `orishu.coupled-entities/v1` | 80 | ID `u64`; position `3×f64`; velocity `3×f64`; flags `u8`; three reserved zero bytes; slot index `u32`; source `f64`; response `f64` |
| 3 | `orishu.forces/v1` | 32 | ID `u64`; Cartesian force `3×f64` |
| 4 | `orishu.entity-ids/v1` | 8 | ID `u64`, for explicit membership sets such as deaths |
| 5 | `orishu.simulation.objects/v1` | 72 | ID `u64`; position `3×f64`; velocity `3×f64`; Dynamics-presence `u8`; seven reserved zero bytes; optional inertial mass `f64` |

The whole-object packet retains static/kinematic objects, including those with no
field coupling. Dynamics presence is derived from the captured optional component,
not an independently editable motion authority. The presence byte is 0 or 1;
absent mass must be positive zero and present mass strictly positive. All objects
sort by ID. The run derives role projections and accepts integrator output only
for its existing dynamic IDs with unchanged inertial masses. Non-numeric authored
components remain separate declared workload data, not erased by this projection.

Coupling flags: bit 0 = Dynamics present, bit 1 = source present, bit 2 = response
present. Other bits are invalid. An absent source/response slot must encode numeric
positive zero. Coupled records sort strictly by `(entity ID, slot index)`; all slots
of one entity must agree on kinematics and Dynamics presence. Dynamics, force and membership
records sort strictly by entity ID. Duplicates and noncanonical order are rejected,
never sorted away, summed or interpreted as a last-write-wins update.

Readers check byte/count policy and checked total length before scanning records.
They validate all records before exposing a `Batch`, then borrow immutable bytes
without allocating. Indexed access is O(1), validation/iteration O(records).
Writers validate first, reserve caller storage once and encode in place; rejection
leaves prior output bytes intact. Default ceilings are 128 MiB and one million
records per packet; callers can supply tighter budgets.

## Fixed-profile reduction

`orishu_runtime::ForceReducer` receives the complete canonical selected-field list,
the committed dynamic entity packet, each field's coupled projection and finished
force packet. It verifies exact field coverage, declared coupling-slot bounds,
dynamic membership/kinematics, and exactly one force for every responding entity.
Missing, duplicate, unexpected or stale projections are errors, not implied zeros.

Field completion order is irrelevant: summation uses ascending field-instance ID
order and checks every intermediate sum for finiteness. The result contains a
net force for every dynamic entity. An explicitly empty selected-field set produces
zero net force, allowing the integrator to provide ordinary inertial motion; it
never permits omission of an actually selected field's output.

Scratch storage is reused across calls. Bounds cover fields, dynamic entities and
aggregate coupled/force records before allocation. Complexity is
O(fields log fields + coupled records log dynamic entities + dynamic entities),
with O(fields + dynamic entities) storage. Errors return no reduced result.
Scientific commit and integration remain outside this helper.

## Instance and validation envelopes

### Resolved configuration and domain

All bound scientific operations now use `orishu.simulation.configuration/v1` and
`orishu.simulation.domain/v1` input schemas (logical count 1). Both use bounded
deterministic CBOR with an explicit `apiVersion`; they are shared authoring/kernel
inputs, not private Euler/gravity encodings. Their syntax is independent of an
installed plugin's language, build system or internal field layout.

`ResolvedConfiguration` contains a strictly ID-sorted `properties` list of
`{id, value}`. Values are tagged as `quantity {valueSI, dimension}`, `boolean
{value}`, or `text {value}`. Shared dimensions use SI base exponents. No expression,
URL, installation reference or implicit executable selection appears in this
resolved artifact. Empty configuration is explicit, not a missing input.

The shared `resolve_configuration` helper compiles borrowed authored expression/
literal inputs and declared defaults using the caller's `VariablesSystem`.
It refuses undeclared/duplicate inputs, missing required properties, wrong types,
wrong dimensions and violated numeric/text bounds. Input order does not select
meaning. It leaves authored source and the variable environment untouched;
document-owned source/provenance still needs to be captured by authoring/export.
Existing captured bytes never re-evaluate when variables/defaults change.
`validate_against` checks captured values against a selected schema without
inserting defaults. Workers must use this check independently during admission.

Configuration limits use the shared plugin policy (by default 256 KiB, 256
properties and 4096-byte text), with canonical byte/depth/value checks before typed
decoding. The compilation helper makes at most one evaluation per property;
each evaluation uses the supplied variables engine's source/dependency/work bounds.
Schema/source parsing also obeys plugin expression bounds. This cold path is
O(properties² + evaluated expression work); it is not invoked per field sample.

`DomainDescriptor` carries fixed-rank `lowerMetres[3]`, `upperMetres[3]` in the
same Cartesian x/y/z world frame as object kinematics, plus `discretization`:
`{kind: "continuous"}` or `{kind: "cartesian-cells", cells: [nx,ny,nz]}`.
No implicit origin translation is permitted. Continuous does not request a grid;
Cartesian cells explicitly cover the box with cell edges at its corners. It does
not prescribe a kernel's opaque memory layout, halo, partition or observer density.
Kernels reject unsupported schemes rather than silently replacing them.

Physical field boundary policies and their values belong to the selected model's
declared configuration (ADR 0023), not to a single global physics rule. For example,
the direct Newtonian reference explicitly accepts `boundary = "isolated"`;
periodic gravity is not produced by ignoring that request. Different fields can
declare different compatible boundary treatments over the shared geometric box.

Domain decoding is bounded to 4096 bytes, depth 8 and 128 structural values. It
rejects empty/inverted/nonrepresentable extents, zero grid axes, overflowing or
over-budget cell products and collapsed floating-point cell spacing. The caller's
`DomainLimits` defaults to 16,777,216 cells and never causes allocation of that
grid. Low-level bound lifecycle calls check arithmetic with a `u64` cell ceiling;
the future run/admission owner must impose its actual allocation/partition budget.
Unsupported meshes/segmented input schemes require an explicit future profile;
opaque kernel state is not restricted to a Cartesian matrix.

These are new versioned execution-input artifacts, **not** an in-place replacement
of existing `orishu-workload::DomainSpec` canonical bytes or Kagami document files.
The selected workload/version integration must reconcile those representations.
In particular, Kagami's current global cell/boundary fields must be preserved or
explicitly rejected/migrated: do not silently discard them when selecting a model,
move the domain origin, or reinterpret periodic intent as isolated gravity.

### Exact instance and validation binding

`InstanceContext` is cold, deterministic CBOR under descriptor schema and
`apiVersion` `orishu.simulation.instance/v1`, with logical count 1. Its explicit
projection includes instance ID, exact original kernel digest, provider-qualified
contribution, scientific contract reference, one execution contract, portable
state/history format, execution profile, captured configuration/domain identities,
canonical coupling and observable tables, declared compute precision and negotiated
bounds. Input identities include schema,
logical count, exact byte length and digest; bytes supplied to setup/validation
must match all of them. No installation path or default provider enters the context.

The coupling table sorts strictly by requirement-slot name. Each entry names an
exact component scientific contract and independent optional source/response
property names with SI dimensions. The compact packet slot indexes this table.
At least one role is required; a packet cannot supply an undeclared role. Dynamics
contexts have no field-coupling table. This does not authorize using a different
component merely because its dimension matches.

The observable table also sorts strictly by requirement-slot name. Each binding
includes the exact scientific observable reference and schema, plus its allowed
quality mask (direct=1, interpolation=2, reconstruction=4). The schema digest must
match the reference; duplicate observable contracts, unknown/empty quality masks
and observables on a Dynamics context are refused. Compute precision is explicitly
`binary32` or `binary64`; binary64 transfer does not upgrade scientific accuracy.

Context decoding bounds bytes (64 KiB), nesting (12), structural values (4096),
text (256 bytes) and coupling slots (64), before exposing typed metadata. Unknown
versions, fields, duplicate keys, invalid roles, noncanonical order/encoding and
zero projection ceilings are refused. Zero state-byte bounds can represent an
explicitly declared empty memoryless history; they never imply missing state is
acceptable. Scientific admission still verifies that every
context value follows from the pinned closure, instance and field configuration.
`StateFormat {id, version}` maps to grant schema `<id>/v<version>` in this profile.

`ValidationInputs` uses descriptor `orishu.simulation.validation/v1`, logical count
1, and a fixed 28-byte header followed by four concatenated byte sections:

| Offset | Bytes | Meaning |
| --- | --- | --- |
| 0 | 4 | ASCII `OSV1` |
| 4 | 8 | Positive finite authored SI timestep, little-endian binary64 |
| 12 | 4 | Canonical instance-context byte length, `u32` |
| 16 | 4 | Domain/discretization byte length, `u32` |
| 20 | 4 | Resolved configuration byte length, `u32` |
| 24 | 4 | Standard entity-projection byte length, `u32` |

Sections follow in the same order with no padding/trailing data. Field validation
uses the coupled-entity packet; Dynamics uses the dynamic-entity packet. Empty
entity sets still carry a valid standard packet. The captured opaque state/history
is supplied separately through `common.load` or `common.restore`, never generated
by the validation envelope. Domain/configuration interpretation follows their
declared schemas; this framing does not turn arbitrary bytes into a shared domain.

The reader checks aggregate extent before slicing/scanning, verifies exact context
equality and domain/configuration digests, then validates the role projection and
slot bounds. Bulk sections are borrowed; context metadata decoding is bounded but
allocates. Encoding reuses caller storage and preserves prior bytes on rejection.
Operation-time allocation/repeated cold-context decoding remains a hot-path gate.

`Sandbox::invoke_field_bound` and `invoke_dynamics_bound` check the compiled
artifact, input identities, state format/bounds and validation envelope before
starting a guest. For stepping, validated `dt` and entity bytes must equal the
actual step inputs. This prevents validating a benign projection then executing
another. Typed `ContextRejection` preserves mismatch categories. The raw lifecycle
methods remain ABI-test surfaces, not an alternative admission path. The bound
methods still do not prove release-closure admission, checkpoint provenance or
whole-run atomicity. Sampling additionally uses the checked packets below.

### Requested-channel sampling

`SampleRequest` uses descriptor `orishu.simulation.sample-request/v1`, logical
count = points. Its little-endian framing is `OSQ1`, metadata byte length (`u32`),
point count (`u32`), canonical CBOR metadata, then 32-byte point rows: request-local
ID (`u64`) and Cartesian world position (`3 × f64` metres). IDs are unique, not
necessarily sorted; positions are finite with canonical zero. Point generation
belongs to the observer, not this contract or the scientific kernel.

Metadata (`orishu.simulation.sample-metadata/v1`) binds request ID, exact field
instance, canonical instance-context digest, requested ordered channel schemas and
an exact field-state input identity. Its source is either an authored-revision
artifact digest or a committed workload digest, immutable run-descriptor digest,
epoch, boundary and nonnegative SI simulation time. These content references are
not a new implementation of run allocation or existing protocol run IDs. The future
run/export authority must construct and lease the corresponding captured artifacts;
merely naming them does not prove provenance, commitment or access rights.

`SampleResponse` uses descriptor `orishu.simulation.sample-response/v1`, logical
count = points. Its prefix is `OSP1`, the 32-byte SHA-256 of the **entire exact
request packet**, then declared compute precision (`u32`, 32 or 64). For each
requested channel, in request order, the following three contiguous arrays follow:

| Array | Element count | Encoding |
| --- | --- | --- |
| Validity | points | `u32`: 0 valid, 1 outside domain, 2 undefined, 3 singular, 4 channel unavailable |
| Quality | points | `u32` mask; nonzero allowed subset for valid cells, zero otherwise |
| Values | points × channel components | finite SI `f64`; row-major matrices, request-point order |

`SampleLayout` derives disjoint checked offsets, lengths and point strides from
the identified request; there is no redundant untrusted offset table. Response
bytes must be retained/routed together with their request and context. The request
digest prevents reusing output for different positions, channels, state or source.
Invalid slots are unspecified and **never exposed as numeric values** by the cell
reader. An unsupported exact contract must report `ChannelUnavailable`, not a zero
or a substituted same-name channel. Supported channels report point invalidity
separately; optional Jacobian failure need not invalidate acceleration/potential.

Default request limits are 4096 points, 16 channels, 8 MiB per packet and 1,048,576
numeric components; checked response-byte products further restrict combinations.
Cold metadata is limited to 64 KiB, depth 16 and 8192 structural values. Readers
validate framing/extents before point scans or count-derived allocations. Reused
ID scratch checks uniqueness in O(points log points). Output values stay in one
caller-reused flat buffer: no heap object per cell. Cold metadata/layout decoding
still allocates O(channels/schema metadata); repeated decode elimination and leased
output pools remain run/observer integration work, not a claimed zero-allocation path.

`SampleOutput` requires every cell exactly once; a failed/duplicate write poisons
completion even if ignored by a guest. Host bound sampling checks schema/count,
exact snapshot bytes, context, output extent and complete response validation on
return. Sampling still instantiates an isolated disposable guest. This protects
the retained state. `FixedRun` now adds bounded committed-field snapshot leases,
source checks and detached sampling; observation cache/recording and product
adapter integration remain work.

## Verification and remaining integration

Tests include an independently spelled byte golden, all truncated prefixes,
wrong kind/version, trailing bytes, count/size overflow, non-finite values,
negative zero, reserved bits, duplicate IDs/slots, inconsistent repeated entity
kinematics and zero-versus-absent coupling semantics. Reducer tests permute field
completion order with cancellation-sensitive floating-point sums, check complete
coverage, reject intermediate overflow, and verify reuse of output capacity.

These schemas do not reinterpret existing workload versions or legacy Component
lifecycle strings. The [reference Newtonian and Euler Components](../plugins/reference/README.md)
now use standard role packets plus bound instance/validation inputs and have coupled
numerical/restart evidence. Their contribution pins in tests are synthetic, not
admitted releases. Shared resolved configuration/domain inputs are also implemented
and used by those kernels. Requested-channel sampling is exercised through the
actual Newtonian Component, including subsets/order, unsupported contracts, per-cell
invalidity and identity substitution rejection. `FixedRun` now owns atomic
single-partition boundaries and field-snapshot leases with real numerical/failure/
restart evidence. Reusable execution storage, document/version integration,
selected workload export and worker admission remain work. Existing lifecycle fixtures use explicitly test-only schemas
and are not numerical proof of these scientific kernels.
