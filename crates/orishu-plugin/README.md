# Shared plugin declarations — X-PLUGIN slice 1

Implemented: pure declarations for the six v1 extension points, release and
scientific identities, bounded JSON/CBOR readers, explicit canonical projections,
root validation and digest-verified contribution payloads. No filesystem,
network, UI, inventory, provider selection or guest execution is linked here.

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
4. Provider eligibility/resolution, cross-provider compatibility, selected
   workload closure, Wasm ABI inspection and numerical validation are later slices.
   An exact external requirement is permitted before its provider is installed.
   Local availability cycles are likewise the resolver's dormant state, not
   automatically root corruption. “Known/verified” never means “available” or
   “executable”.

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
is a later packaging-shell bound. Products and sums use checked arithmetic.

JSON Schema describes structural shape and default ceilings, not every acceptance
invariant. JSON string lengths count characters, while Rust enforces UTF-8 bytes;
Rust additionally checks uniqueness, cross-references, role dimensions and budgets.
Errors carry a stable code and bounded field subject/message, never arbitrary
rejected bytes. Deep serde schema mismatches currently use `$` as their subject;
adapters must not invent a more precise location. No parser error is run admission.

## Verification and next slice

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

Next: X-PLUGIN slice 2, deterministic contribution availability and provider
resolution. Package ZIP validation/repacking, installation transactions, document
lock/selection representation, authoring projection, selected-workload evidence,
WIT bindings, guest budgets and real execution proof remain explicitly unimplemented.
