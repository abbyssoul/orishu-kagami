# Shared plugin declarations — X-PLUGIN slice 1

The strict `archive` framing reader/writer is shared by `.okplugin` bundles and
scientific document containers and portable workloads. The reader checks CRC/framing; each semantic
consumer independently checks its exact SHA-256 closure. `VerifiedDeclarations`
supports offline document restoration from release evidence and selected payloads,
but is not a `VerifiedSelection`: workload execution still requires selected code.

Implemented: pure declarations for the six v1 extension points, release and
scientific identities, bounded JSON/CBOR readers, explicit canonical projections,
root validation and digest-verified contribution payloads. No filesystem,
network, UI, inventory IO or guest execution is linked here. The pure
`resolution` module now supplies verified release snapshots, exact transitive
provider selection and revision-bound candidate pagination; it performs no installs.
The pure `bundle` module validates and packs caller-owned `.okplugin` stored-ZIP
bytes. This is not source-directory loading, an installer, or Wasm ABI admission.
The pure `execution` module supplies [standard scientific bulk IO](../../docs/scientific-bulk-io.md):
bounded borrowed Dynamics/coupling/force packets with explicit SI semantics and
canonical binary framing. It owns no equation, integrator or entity allocator.
It also supplies exact instance/validation envelopes, resolved dimensioned
configuration/domain inputs and requested-channel sampling packets with flat
values/validity/quality buffers. These are bounded data contracts, not run
provenance, snapshot lease or workload-admission authorities.

The [accepted contract](../../docs/plugin-contract-v1-draft.md) owns semantics.
[JSON Schema](schema/plugin-v1.schema.json) supplies the root and six payload
shapes; select the payload `$defs` entry using its external extension-point ID.
The [fixture](tests/fixtures/contract-v1.json) includes all six shapes, independent
gravity vocabulary, classical/alternative field-model providers, Euler, exact
canonical hex and real release/scientific/artifact digests. Kernel bytes in these
fixtures are **inert test data**, not a working solver or valid Wasm component.

## Acceptance layers

1. `Release` and payload structs are raw authoring declarations. Their serde
   support is not an untrusted-input acceptance API. Use `release_from_json`,
   `release_from_cbor`, `payload_from_json` or `payload_from_cbor` with `Limits`.
2. `Release::validate` checks common metadata, bounds, uniqueness and local
   descriptor references before fetching any bytes. It returns a borrowing
   `ValidatedRelease`, not a runnable workload or verified package.
3. `verify_payload` checks exact bytes and one payload's schema/envelope agreement.
   `verify_all` checks every declared artifact and matches understood local
   dependency identities, hashing each target's scientific declaration once.
   Unknown payloads are integrity-checked but neither parsed nor activated.
4. `resolution::Inventory` evaluates exact provider eligibility and produces a
   complete pinned selection or bounded failure, without mutating authoring intent.
   Exact cross-provider role checks and selected byte closure are subsequently
   rechecked by `selected`; Wasm ABI and numerical validation belong to the runtime.
   An exact external requirement is permitted before its provider is installed.
   Local availability cycles are likewise the resolver's dormant state, not
   automatically root corruption. “Known/verified” never means “available” or
   “executable”.
5. `bundle::read` establishes archive framing, exact root/blob closure, CRCs and
   digest-verified declarations before returning borrowed blobs and a verified
   release. `bundle::pack` emits only declared blobs, with deterministic framing;
   additional blobs in the caller's cache are not exported. `BundleLimits` bounds
   aggregate archive bytes and physical entry count before work/allocation.
6. `selected::{compile, verify}` establishes exact selected scientific closure:
   release evidence, selected payloads, transitive exact bindings and selected code.
   It excludes unused code/icons/docs, checks dependency roles and required channels,
   and does not consult installed defaults or activate opaque contributions. See
   [selected closure](../../docs/plugin-selected-closure.md). This is not workload
   profile adoption, ABI validation or worker admission.
   `selected::build_context` projects exact selected vocabulary into instance
   metadata (coupling roles, dimensions, channels, state format and profile).
   Callers still supply explicit input identities, precision, bounds and sampling
   quality allowances; the independent `verify_context` checks the result.
7. `workload::{compile, verify}` binds a v3 root to the exact fixed scientific graph,
   selected closure and captured configuration/domain/object/coupling/state inputs.
   Execution-v2 additionally verifies `SceneDefinition` component values and
   non-executable authoring evidence against the complete numerical projection.
   Additional data components survive, unused contributions still refuse, and
   template fingerprints never become fetch edges. Execution-v1 bytes are unchanged.
   Actual Component compilation and numerical validation are performed by
   `orishu_runtime::admit`; see [the profile](../../docs/workload-v3.md). Neither
   library function implements Kagami document export or worker endpoints.

The initial archive profile also rejects comments, extra fields and streaming data
descriptors. Central records may be reordered and matching local/central timestamps
may differ between packages. These details do not affect release identity. Physical
entries must cover the local area exactly with no prefix, gaps or overlap. Range
verification precedes CRC scanning to keep malformed overlapping ranges from
multiplying content work. No archive paths are extracted to a filesystem.

`verify_all` takes borrowed caller-held blobs. It does not fetch, extract or copy
kernel/input buffers. Verification costs O(declared bytes), plus bounded schema
encoding and O((contributions + dependency edges) log(contributions)) index work.
The future IO shell must bound acquisition before calling it; it must not gather
arbitrary bytes merely to discover that a descriptor was over budget.

## Concrete schema details

These are the slice-1 spelling/layout refinements of the accepted schema notation:

- Each known payload is `{scientific, presentation?}`. Scientific declarations
  carry `name`, nonzero `version` and exact `requirements` keyed by slot. Payloads
  include their schema version through the external extension-point ID.
- `requirements` and `constants` are sets, sorted by slot/ID for identity.
  Release contributions sort by local ID, artifacts by digest bytes. Property,
  domain-requirement and channel/axis lists retain their declared order; duplicate
  IDs/slots are refused, never removed during sorting.
- Quantities use the shared seven SI exponents. Property defaults retain
  `defaultExpression`; syntax is checked using the bounded shared variables
  parser, without resolving symbols or evaluating expressions. Optional inclusive
  `minimumSI`/`maximumSI` constrain resolved values in later instance validation.
  Boolean/text defaults are literals; text has `maxBytes`.
- A component's `bindings` names required scalar quantity properties. Dynamics
  binds only `inertialMass` with mass dimension. Field coupling binds source,
  response or both, with independently declared dimensions. Data binds neither.
- Field families specify `domainDimension: 3`, domain/discretization requirement
  identifiers and observable requirement slots. Their concrete solver-specific
  admissibility is not guessed from these identifiers by this crate.
- Observable `shape.kind` is scalar/vector/matrix. `axes` contains one descriptive
  axis meaning per vector rank or matrix rank (zero/one/two), not one string per
  element; for a spatial vector, e.g. `axes: ["x,y,z"]`. Matrices are row-major.
  Meaning, dimension, frame and conventions participate in scientific identity.
- Models name a state format, kernel artifact, exact execution contract/profile,
  configuration and explicit `fieldTimeConvention`/`maxStateBytes`. Integrators
  declare history format, `samples`, `maxBytesPerEntity` and kernel-owned
  `coldStart`. These declared storage bounds are not allocation requests or
  permission to bypass a later runtime budget.
- Digests are lowercase `sha256:` text in these projections. Hashing reuses
  `orishu-workload` primitives; it introduces no codec changes. The existing
  workload envelope/graph/fixtures and legacy document/catalog IDs are unchanged.

## Bounded parsing and diagnostics

Input bytes are checked before parsing. Both readers apply depth and total-value
budgets. CBOR uses the existing shared decoder's bounded intermediate tree, then
checks field-specific collection/text bounds before typed deserialization. JSON
checks collection limits during reading, before deserializing the rejected next
element. Escaped JSON strings may use the codec's scratch storage, bounded by the
whole-input byte ceiling. Neither path is an unbounded serde convenience wrapper.

Programmatically built declarations pass the same schema/encoding budgets before
acceptance or hashing. The hard structural depth ceiling is 64 even if a caller
requests more; default is 32. Additional structural defaults are 65,536 values,
4,096 UTF-8 bytes per string, 256 schema list items, 32 fields per object, and shared expression parser
bounds. These supplement the accepted contract's root/payload/artifact ceilings.
An aggregate declared-byte budget counts each unique digest once; archive overhead
is a separate `BundleLimits` bound. Products and sums use checked arithmetic.

JSON Schema describes structural shape and default ceilings, not every acceptance
invariant. JSON string lengths count characters, while Rust enforces UTF-8 bytes;
Rust additionally checks uniqueness, cross-references, role dimensions and budgets.
Errors carry a stable code and bounded field subject/message, never arbitrary
rejected bytes. Deep serde schema mismatches currently use `$` as their subject;
adapters must not invent a more precise location. No parser error is run admission.

## Verification and next slice

`workload::bundle::{pack, read}` now implements the
[portable workload v1 encoding](../../docs/workload-bundle-v1.md) over a v3
scientific root and its exact closure. It excludes unrelated cache bytes on pack,
rejects extra/missing physical blobs on read and verifies independently of any
installed plugin inventory. No IO or guest execution occurs in this codec.

```sh
cargo test --locked -p orishu-plugin --all-targets
cargo test --locked -p orishu-plugin --doc
cargo clippy --locked -p orishu-plugin --all-targets -- -D warnings
cargo test --locked -p orishu-workload --all-targets
cargo run --locked -p orishu-plugin --example contract_fixture
```

The last command emits reviewable fixture JSON to stdout, never writes or installs
anything. Do not refresh golden vectors without reviewing semantic/byte changes.
Tests independently decode them with `ciborium` (test-only, already present in the
workspace lockfile). Runtime dependency tests inspect Cargo's resolved normal/build
graph, including transitive dependencies.

Provider resolution now has focused serialized/public-interface evidence in
`tests/resolution.rs`. Bundle tests include malformed headers, truncated archives,
exact closure, CRC/digest corruption, limits, identity-invariant repacking and an
independent Python `zipfile` golden vector. `tests/selected.rs` checks independent
selected-byte verification and exclusion of unused artifacts. Initial source and
inventory IO now lives in Kagami's plugin authority; real WIT/guest execution and
atomic fixed-profile ownership live in `orishu-runtime`. See the
[delivery ledger](../../docs/tasks/x-plugin-delivery-ledger.md) for verified progress
and remaining management hardening, document/selection adoption, versioned workload
export/admission, reusable runtime storage and worker/product integration.

## Benchmarks

```sh
cargo bench -p orishu-plugin --bench plugin
cargo run --release -p orishu-plugin --example profile_plugin --features dhat
```

The benchmark measures the scientific bulk-IO packet path — `encode_batch`,
`Batch::read`, and iteration — over synthetic `Force` and `ObjectState` records,
scaling with the record count (overridable via `ORISHU_PLUGIN_BENCH_COUNTS`).
The `dhat` example reports the allocation cost of the same phases and confirms
the crate's contract that reading and iterating a validated packet are
allocation-free.
