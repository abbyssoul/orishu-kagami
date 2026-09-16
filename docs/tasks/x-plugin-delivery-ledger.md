# X-PLUGIN end-to-end delivery ledger

Status: **active implementation; not complete**. This ledger preserves the full
requested outcome across implementation passes; it does not replace or narrow
[X-PLUGIN](define-and-implement-plugin-contract.md).

## Required end state and evidence

- Kagami manages local plugins through the accepted CLI commands, a shared
  authority and product adapters. Prove real source/bundle validation, packing,
  inspect/install/list/update/default/enable/disable/remove and process-only
  overrides, including concurrent writers, crash safety and open-reference leases.
- Authoring uses plugin-contributed schemas and exact providers. Prove unavailable
  contributions preserve intent, ambiguities return choices, installed updates do
  not rewrite experiments, and UI/MCP use the same authority. Coordinate with the
  separately ongoing MCP work rather than overwriting its startup/server changes.
- Kagami exports a self-contained selected workload closure. Prove independent
  vocabulary/kernel providers, exclusion of unused executable payloads, captured
  field/entity/history state, exact provenance and no catalog/install-path lookup
  by a worker. Preserve existing workload/document versions with explicit migration.
- The shared sandbox engine actually executes the pinned field and integrator
  components, both locally and in an Orishu worker. Prove initialization, validation,
  fixed field-force/integration steps, committed observations, bounded sampling and
  checkpoint/restore; traps, bad output and exhaustion cannot commit partial state.
- Demonstrate Newtonian gravity plus a compatible dynamics integrator using real
  independently compiled Wasm components. Use Field CAD's dynamics/Newtonian code as
  a historical reference, not a substitute for the accepted common ABI or host.
  Prove an analytic/reference result and restart equivalence through real APIs.
- Update protocol/versioned ABI, manifests, tasks, roadmap and user documentation
  to match delivered behavior. Do not equate package validation with executable
  admission, or formation readiness with workload execution.

The full goal is complete only when this complete path is verified. Registry and
discovery, native visual plugins and expanded force-stage integrators remain the
accepted post-MVP exclusions. Artifact-admin policy retains its separate backlog;
it is not a worker-side plugin manager requirement.

## Current verified progress

### Declaration layer

Slice 1 exists in `crates/orishu-plugin`: bounded declarations, exact identities,
canonical release/scientific encoding, six schemas and verified payloads. Its fixed
fixtures intentionally contain inert kernel bytes, not executable Wasm evidence.

### Provider resolution — 2026-09-16

Implemented `orishu_plugin::resolution`: owned verified release declarations,
validated revisioned inventory snapshots, deterministic transitive resolution,
explicit provider bindings, stale-revision responses, bounded ambiguity diagnostics
and candidate pagination. Existing pins never substitute another release; automatic
eligibility uses enabled defaults plus explicitly selected providers. Installation
order and dependency-search order cannot silently select physics.

Tests exercise independent dormant-before-vocabulary providers, ambiguity and retry,
side-by-side defaults, missing/disabled/incompatible pins, opaque dependencies with
independent usable contributions, invalid snapshots, bounded work/depth/diagnostics,
candidate pages and real JSON request/result round trips. Selection remains raw
authoring intent that must be revalidated; this is not a new workload wire version.

Validation at this checkpoint: all 30 `orishu-plugin` tests and its all-target
Clippy check passed; documentation links/checks passed. The concurrently modified
Kagami/MCP application was not claimed verified by these pure-crate checks.

## Next implementation work

Immediate product integration priority after the startup/initializer checkpoints below:
extend the implemented guarded complete-setup creation/replacement and field-reset
adapter with targeted parameter edits, dependency-choice dialogs and object-edit
history regeneration (whole-setup schema-driven editing/copy is implemented);
finish other shell pending-effect/IO reservations, then
connect window export and worker delivery/run control. Headless captured-file
export is now implemented; see the portable export checkpoint below. Startup vocabulary and
process-only overrides now reach the actual app authority.
Worker integration now has the formation-owner reservation and off-owner validated
admission handoff from ADR 0028. Actual portable closures reach the shared sandbox,
with parent-linked guest cancellation and reserved owner confirmation. The owner
now allocates canonical run descriptors and non-reusable workload epochs, with
real worker bootstrap/restart evidence (ADR 0029). Lease-bound complete-body IO
with expected-root checks is now implemented. Next connect authenticated request
framing, durable identified command receipts and public daemon adoption. The
retained executor now uses a shared commit gate for owner-serialized initial/step/
stop publication and bounded field leases; see the retained-run checkpoint below.
A confirmed admission or read-only fence check alone is still not commit.
Keep public workload routes/capability advertisement disabled until that path is
verified with real exported bytes and lifecycle cancellation.
Installed selections can now feed the shared sandbox through the app's initializer
without hand-assembled reference metadata. Explicit captured-field reinitialization
and whole-setup creation/replacement now adopt through a guarded window effect.
Future configuration/MCP paths must use that gate rather than adopting candidates directly.
The fixed-state reference pair can exercise that real
journey now; do not substitute more isolated fixtures for application adoption.
The remaining full-goal gates below still apply.

1. Complete source tooling (symbolic local references/topological compilation),
   inventory hardening (origin metadata, every crash boundary), non-Unix secure IO.
2. Finish explicit dependency selection and real open-document leases; add UI/MCP
   management and live inventory refresh through the same local authority.
   Inspect concurrent MCP edits before modifying its server/command surface.
3. Extend the WIT/grant/field/Dynamics lifecycle host below: close aggregate/JIT budget
   and attribution gates, replace the implemented fixed owner's disposable stores
   and projection allocations with bounded reusable storage. Complete membership and
   variable output sizing, then versioned scientific workload selection and
   authoring state capture. The pure selected-closure compiler/verifier below is
   implemented, as are the v3 root/captured-definition codecs and context checks;
   fixed scientific-profile assembly and actual selected Component admission are
   now implemented. An internal document-to-numerical-workload bridge now reaches
   real admission from a reopened v4 document. Scene-bearing execution v2 now
   preserves shared entity composition and source evidence. Headless portable
   export is now implemented. Guarded Unix scientific creation/replacement and
   field reset are implemented. Emitter export, worker transfer, remaining
   authoring adapters and worker adoption remain.
   Do not insert kernel-specific
   adapters into the host/compiler merely to make the reference path pass.
4. Workload export/submission and worker admission/execution, with reference,
   checkpoint and malformed/hostile end-to-end evidence. Finish user-facing parity
   and documentation against this actual path, not against mocks or plan claims.

The accepted stored-ZIP profile is specified in the contract. Format implementation
should follow the [PKWARE ZIP specification](https://pkware.cachefly.net/webdocs/casestudies/APPNOTE.TXT)
and explicitly test local/central agreement and bounds; no archive metadata becomes
scientific identity. Initial source/inventory IO is recorded below; this is not
evidence that all management adapters or execution have been delivered.

## Stored-ZIP byte codec — 2026-09-16

Implemented `orishu_plugin::bundle::{read, pack}` without filesystem IO,
extraction, dependencies or execution. The reader validates bounded framing,
local/central agreement, regular safe entry paths, contiguous non-overlapping
ranges, CRCs, exact root/blob closure and all declared digests. It verifies ranges
before scanning content. Packing omits undeclared caller-cache blobs and emits a
deterministic archive without making timestamps/order part of release identity.

Tests cover all truncated prefixes, hostile size/count/offset claims, unsupported
features, links, paths, duplicate records, prefix data, missing/renamed blobs,
missing roots, corruption with and without repaired CRCs, exact budget boundaries
and reordered/re-timestamped packages. A Python stdlib `zipfile` golden fixture
independently verifies reader compatibility and writer framing after normalizing
only timestamp/permission/UTF8-flag metadata. Fixture kernels remain inert.

Validation: all 41 plugin tests, its doctest and all-target Clippy passed; scoped
formatting, `git diff --check` and `make docs-check` (172 Markdown files) passed.
No new dependency or workload/document identity change. These checks do not claim
verification of the concurrent Kagami/MCP application changes.

This is a partial slice-3 delivery, not CLI packing or installation. The complete
management → selected export → sandbox execution goal remains active.

## Initial Unix source/inventory/CLI — 2026-09-16

Implemented app-local `apps/kagami/src/plugins`:

- `Package` reads a bounded `orishu.plugin-source/v1` directory or stored bundle,
  verifies declarations/digests and emits a deterministic bundle. Source inputs
  are low-level externally built exact payloads; source-local symbolic references
  are not yet compiled automatically. Output publication refuses existing names.
- `PluginStore` maintains `orishu.plugin-inventory/v1`, expected revisions, exact
  side-by-side releases, explicit defaults and logical enablement. Blobs/roots are
  durable before index publication. The cache is append-only; removal de-registers.
- Directory-relative no-follow IO uses app-only pinned `rustix` 1.1.4 (already in
  the lockfile); no IO dependency entered `orishu-plugin`. OS shared leases survive
  acknowledged removal; nonblocking process/cross-process locks serialize writers.
- The actual `kagami plugin` binary dispatches validate/pack/inspect/install/update/
  list/set-default/enable/disable/remove, `--json`, explicit inventory and expected
  revision/release options, without opening a window or MCP listener. A cold store
  resolver supports process-only overrides, but startup/authoring adoption is pending.

Verification: 10 real-filesystem/CLI integration tests, including a child-process
lease and a separately locked process, plus the source collection-reader unit test.
Tests cover source packing, exact release identity, default/update/disable behavior,
restart, stale writes, concurrent writers, corrupted cache, ignored interrupted
staging, source/symlink escapes and structured CLI failure outcomes. All 64 Kagami
tests pass when loopback networking is available (the initial sandbox run refused
the unrelated MCP tests' TCP binds; authorized rerun passed all eight MCP wire tests).
Kagami all-target Clippy passed. No window smoke was needed for headless dispatch;
there is no claim of plugin UI integration or real physics yet.
The 41 shared plugin tests also passed with the app dependency changes;
`cargo fmt --all -- --check`, Kagami doctests (none defined), `git diff --check`
and `make docs-check` (173 Markdown files) passed. Full workspace runtime tests
have not been claimed by this checkpoint.

Remaining management gaps are explicit in [tooling documentation](../plugin-authoring-tools.md):
source-local symbolic compilation, origin capture, non-Unix IO, exhaustive crash
injection, startup/UI/MCP adoption and real open-document lease integration. Nothing
in this checkpoint closes the full goal or makes the inert test kernels executable.

## Executable Component foundation — 2026-09-16

Added shared WIT in `orishu-plugin/wit`, generated host bindings for both scientific
worlds, and `orishu-runtime` as the common deep sandbox boundary. Wasmtime 48.0.2
is pinned with minimal synchronous features, fuel/memory/table/stack policy and no
WASI. Borrowed input/candidate resources enforce ranges, contiguous coverage,
exactly-once finish, revocation, aggregate host-call/transfer bounds and poisoned
invalid outputs. Isolated field initialization runs actual Component code and
returns a finished candidate without writing any document/run.

A separately locked Rust `wasm32-unknown-unknown` guest fixture plus checked-in
Component bytes exercises successful nonzero initialization, infinite-loop fuel,
trap, omitted/partial finish and invalid-write behavior. It is a contract/security
fixture, not a Newtonian solver. [ABI checkpoint](../runtime-component-abi.md)
records precise logical-contract/WIT mapping and mandatory remaining security and
integration gates. In particular, current dynamic-list lifting precedes the host's
chunk check; its memory ceiling is not evidence of a pre-lift chunk ceiling.

The worker still advertises no scientific execution capability; neither app uses
this engine yet. No workload/document format identity has been silently migrated.
The full management → authoring/export → actual physics execution goal is active.

Verification: five runtime tests (including three actual-Component integration
tests), runtime all-target Clippy, standalone guest Wasm build/Clippy, all 41 shared
plugin tests, workspace formatting and documentation checks passed. The shared
dependency check initially required fetching the new engine's non-host `mach2`
dependency for offline Cargo metadata; it passed after `cargo fetch --locked`.
No full-workspace runtime or application integration result is claimed here.

## Bounded lifting, interruption and field lifecycle — 2026-09-16

Replaced dynamic guest-to-host writes with fixed 64 KiB WIT frames and an explicit
logical prefix length. Canonical decoding is bounded before the host callback;
padding is not data but counts toward physical transfer work. Invalid logical
lengths poison candidates even when the guest ignores the error. Rebuilt the
independent Component fixture against this development ABI; no deployed workload
format or identity was changed.

Added per-operation deadlines and independently cancellable tokens. Fuel, guest
memory/table bounds and epoch interruption apply from instantiation/start through
setup, load/validation, requested operation and close. Host calls and extraction
check the same deadline. A shared engine's heartbeat never cancels unrelated
operations. Compiler time and aggregate concurrent resources remain separate gates.

`Sandbox::invoke_field` now exercises initialization, explicit-state load and
numerical validation, field advance, isolated snapshot sampling, checkpoint and
restore through actual Component exports. Advance returns finished field and force
candidates together or returns no candidates. Load/restore never initializes natural
defaults; guest rejections preserve typed codes and timestep advice must be positive
and finite. Every call uses a disposable guest/store. This is not yet the reusable
hot-step executor, whole-run restart implementation or scientific admission.

Eight runtime tests pass (three grant tests and five Component integration tests).
They cover physical-frame accounting, ignored excessive writes, fuel/timeout,
independent cancellation, lifecycle ordering, checkpoint bytes, valid/invalid advice,
load/validation/restore/close failures, incomplete forces after completed field
output and sampling failure after completed output. Runtime and standalone guest
Clippy pass; the locked standalone Wasm guest build, all 41 shared plugin tests,
plugin/runtime doctests, workspace formatting, `git diff --check` and documentation
checks pass. These remain security/ABI fixture
results, not gravity accuracy or end-to-end worker execution evidence.

The next scientific slice needs the Dynamics invocation paths and shared portable
entity/force/history formats, followed by real Newtonian/Euler Components and the
fixed-profile atomic coordinator. Keep the full selected authoring/export/worker
delivery goal active; no application integration is implied by this checkpoint.

## Dynamics lifecycle and explicit history — 2026-09-16

Implemented `Sandbox::invoke_dynamics` with initialization of history, loaded-state
validation, integration, explicit birth/death history transition, checkpoint and
restore. Both worlds share exactly the imported WIT resource/type interfaces,
typed rejection/advice checks, fuel and interruption policy. Common semantics live
in a shared host module rather than belonging to either scientific role. Integration
returns entity and history candidates together; all failure paths drop the isolated
operation state and return neither candidate. Membership scheduling remains the
future fixed-profile coordinator's responsibility.

Added an independently compiled Dynamics Component to the separately locked guest
workspace. Its test-only sorted-byte-ID/counter history is intentionally not an
Euler integrator or a product scientific format. Actual Component tests prove
history initialization, explicit empty history, integration counter evolution,
fresh newborn history, retired history removal and surviving history preservation.
Checkpoint/restore preserves next-step output, including a mode that would loop if
initialization were called instead of restoring supplied state. Missing/malformed
history, duplicate births, unknown deaths, incompatible roles, numerical rejection,
invalid advice, traps, close failure and incomplete history after finished entity
output all fail without returning partial candidates.

Verification: all 11 runtime tests, runtime all-target Clippy, both standalone
Wasm guest builds/Clippy/formatting, runtime doctests, the shared plugin dependency
boundary test, workspace formatting, `git diff --check` and documentation checks
pass. An initial host test invocation ran before the new generated Component was
published and failed to find that fixture; after generation completed, the exact
checked-in bytes passed all tests. No worker/application integration or scientific
accuracy is inferred from these fixture results.

Next priority is shared bounded portable scientific entity/coupling/force formats,
real independently compiled Newtonian and fixed-profile-compatible integrator
kernels, and their deterministic atomic step coordinator. The raw lifecycle API
does not admit a workload, bind buffer metadata to selected scientific contracts,
provide a reusable hot-step executor, or deliver the full goal yet.

## Scientific bulk projections and force reduction — 2026-09-16

Added pure `orishu_plugin::execution` standard packets for dynamic entities,
field-coupled entity slots and forces. The [byte specification](../scientific-bulk-io.md)
defines explicit little-endian versioned framing, SI semantics, strict record
ordering, finite/canonical scalars, positive inertial mass and independently present
source/response values. Multiple coupling slots remain distinct per entity; they
are not collapsed into one charge or silently summed by the host. Slot indices
require the selected model's exact descriptor table, whose workload integration
remains work. Derived Dynamics presence is not an authoring motion-authority flag.

Packet readers check byte/count/extent budgets before scanning, validate every
record before exposing a borrowed allocation-free view, and provide constant-time
indexed reads. Writers validate first and reuse caller-owned storage without changing
old output bytes on rejection. `Buffer::scientific_batch` additionally verifies
host descriptor schema/count agreement; it does not prove workload/run binding.

The runtime's reusable `ForceReducer` checks the complete canonical selected-field
set, coupling-slot bounds, dynamic membership/kinematics and exactly one force per
responding entity from each field. It reduces in stable field-instance order,
regardless of completion order, rejecting missing/extra forces and non-finite
intermediate sums. Multiple response slots require one kernel-combined force, not
duplicate host addition. An empty selected-field set yields zero net force for
ordinary inertial motion. No reduction result commits scientific state.

Verification: all 46 shared plugin tests and 16 runtime tests pass, along with both
crates' all-target Clippy and doctests, workspace formatting, `git diff --check`
and documentation checks (177 Markdown files). The plugin crate also passes a
locked `wasm32-unknown-unknown` library check, so an external kernel can use the same
codec without acquiring the runtime. New tests cover a hand-spelled byte golden,
hostile framing/counts/values, zero-versus-absent strengths, independent repeated
slots, descriptor mismatches, all six completion permutations for cancellation-
sensitive sums, exact coverage, aggregate limits and retained output capacity.

No existing workload canonical bytes or application/MCP files changed in this
slice. The next priority is **real** independently compiled Newtonian and compatible
integrator kernels using these packets, followed by the fixed-profile atomic
coordinator. Test-only lifecycle counters/copy kernels must not substitute for
numerical execution evidence. Selected workload context, authoring capture/export,
worker execution and remaining management adapters still belong to the full goal.

## Real classical Euler Component — 2026-09-16

Added the separately locked [reference plugin build workspace](../../plugins/reference/README.md)
with `orishu-reference-symplectic-euler`, independently compiled to a Dynamics
Component using the public WIT and scientific packet crate. Its kick-then-drift
formula is inside the guest, not substituted by native host code. Field CAD's
ownership/order informed it; it is explicitly **classical**, not a port or claim of
its relativistic momentum/Verlet machinery. The migration record captures that choice.

The kernel validates configuration/profile/timestep, consumes complete entity/force
packets, checks finite computed velocity/position and keeps explicit membership
history. It has no numerical look-back requirement; new membership history is
created only by initialization or validated birth/death transitions. Deaths now use
the standard `orishu.entity-ids/v1` packet (kind 4). Checkpoint/restore preserves
history rather than initializing from the supplied entities. Membership validation
precedes result allocation, including unknown deaths and overlapping births/deaths.

Five tests execute the actual Component: inertial mass and kick-before-drift,
equal/opposite-force momentum and zero-force drift, constant-force first-order
convergence, numerical restart into a fresh engine, births/deaths, explicit empty
membership, invalid dt/coverage/history and overflow refusal. At total time one,
constant acceleration one gives position 0.55 for ten steps and 0.525 for twenty;
the error against exact position 0.5 halves, matching the declared first order.

Verification: all 47 shared plugin tests and 21 runtime tests pass, as do host
all-target Clippy, the locked standalone Wasm build/Clippy, both workspace formatting
checks, plugin/runtime doctests, documentation checks (178 Markdown files) and
`git diff --check`. The initial external build exposed a WIT-relative-path mistake
and generated export trampolines conflicting with a blanket unsafe prohibition;
both were corrected before the verified build. Handwritten guest code denies unsafe;
only generated ABI exports have a scoped exception.

This is genuine numerical evidence, not a completed installable release or workload.
The setup input is still a bootstrap profile adapter: replace/adapt it to the full
selected workload `InstanceContext` before admission. Bounded operation-sized guest
allocations and disposable host stores still need hot-path reuse. Next priority is
the real Newtonian field Component, coupled analytic/restart evidence and the atomic
coordinator, then complete selected manifests/context, authoring/export and worker
execution. The full goal remains active; no application/MCP files changed here.

## Real Newtonian Component and bound validation — 2026-09-16

Added `plugins/reference/newtonian`, independently compiled against the public
Field WIT. Field CAD's direct point-source/exclusion/potential/Jacobian design
informed this explicit classical reference; no native plugin registry or host-side
physics was copied. It owns portable bounded source state, initializes without
entities, uses independent source/response masses, excludes only identical-ID
self-force and emits complete force rows. Its source snapshot uses the committed
input positions for the declared field phase, not the integrator's next positions.

The real Component samples direct acceleration, potential and row-major spatial
Jacobian with separate validity/quality per channel. Query IDs remain in arbitrary
request order. Singular/outside points are invalid; optional Jacobian overflow
does not reject finite force or other channels. The reference's three-channel
query/response, configuration and analytic domain encodings are still bootstrap
formats: adopt generic requested-channel/configuration/domain contracts before
product packaging. Uniform-sphere interiors and other source shapes are not hidden
approximations inside this point-source model.

Removed profile-only setup and Euler/Newtonian-specific validation envelopes.
The pure contract now supplies bounded deterministic-CBOR `InstanceContext` and
borrowed `ValidationInputs` packets. They bind exact kernel/provider/scientific
references, state/history format, profile, captured domain/configuration identities,
canonical dimensioned source/response slot tables and instance limits. Validation
has one common framing containing context, domain, configuration and the declared
standard entity projection. Unknown/duplicate/noncanonical context data, hostile
lengths, changed identities, invalid dt and undeclared coupling roles are refused.

`Sandbox::invoke_field_bound` / `invoke_dynamics_bound` preflight compiled artifact,
configuration/domain identity, opaque-state format/bounds, standard output extents
and validation inputs. For stepping, the validated dt and entity bytes must be the
ones actually executed. Structured `ContextRejection` reports mismatch categories.
Both real references consume this path. Low-level lifecycle methods remain an ABI
fixture surface, not an independent admission route. Synthetic test contribution
pins are **not** evidence of release-closure admission.

Numerical tests prove analytic Newtonian force/acceleration/potential/Jacobian,
independent inertial/response mass, point exclusion, malformed state/domain/config
refusal and natural-state preservation. A test-only composition invokes actual
Newtonian → `ForceReducer` → actual Euler, then proves identical next-step candidates
after checkpoint/restore into a fresh engine. It is not an atomic product run owner.
Bound-host tests reject switched artifacts/providers/configuration, a validated
different timestep/entity projection, wrong descriptor counts and insufficient
state bounds before returning any candidate.

Next: generic resolved configuration/domain and requested-channel R4 formats,
then an actual atomic fixed-profile run owner with reusable stores/buffers and
selected descriptor/closure admission. Captured authoring export and independent
worker execution must adopt that same owner; management/startup/UI/MCP and remaining
security/attribution gates still belong to the full goal. No application/MCP source
was changed in this checkpoint. No existing workload/document version was silently
reinterpreted; only development reference artifacts were rebuilt against new input
schemas. The full X-PLUGIN goal remains active, not complete.

Verification for this checkpoint: all 51 shared plugin tests and 27 runtime tests,
both crates' doctests and all-target Clippy, the independently locked Wasm workspace
build/Clippy, workspace/reference formatting, documentation checks (178 Markdown
files) and `git diff --check` pass. The first new context fixture used serde's owned
JSON-value adapter, which cannot supply the digest type's borrowed string; changing
the fixture to the real string reader fixed that test construction failure. No
full-workspace application, UI smoke or worker integration result is claimed.

## Shared resolved configuration and domain — 2026-09-16

Implemented `ResolvedConfiguration`, its bounded canonical reader/writer and
schema validator, plus `resolve_configuration` over the existing shared variables
engine. Declared authored/default expressions resolve once to dimensioned SI
quantities; booleans/text remain explicit typed values. Unknown/duplicate inputs,
missing required values, wrong dimensions/types, violated ranges/text bounds and
expression budget failures return structured errors without changing input source
or the variable environment. Worker-side validation never inserts defaults or
re-evaluates captured values after defaults/variables change.

Added bounded `DomainDescriptor` input artifacts with explicit lower/upper SI
corners and continuous or Cartesian-cell spatial schemes. Grid counts, products,
extent/spacing representability and caller cell budgets are checked without grid
allocation. Physical boundary conditions remain each model's declared configuration
per ADR 0023, not an implied global policy across fields. The first domain input
profile does not implement mesh/segmented inputs; kernel-owned field state remains
opaque and need not be a Cartesian matrix.

Rebuilt both real reference Components to use these shared inputs. Removed their
`OEC1`/`OGC1` configuration and `OGD1` domain bootstrap formats. Euler explicitly
reads dimensionless integral capacity; Newtonian additionally reads dimensioned G,
exclusion radius and `boundary = "isolated"`. Newtonian refuses periodic boundaries,
wrong G dimensions, fractional capacity and a Cartesian-grid request, rather than
claiming that direct point-source computation honored them. Its opaque field state
continues to bind the full captured continuous-domain descriptor on load/restore.
Bound host operations independently parse shared configuration/domain structure.

These are new versioned execution-input artifacts, not a silent document/workload
migration. The current Kagami global cell/boundary fields and old workload domain
must be preserved or explicitly rejected/migrated at the selected export seam.
Comments/task gaps now identify that obligation; persisted authoring behavior has
not changed. The separately ongoing MCP implementation was not edited.

Next priority: requested-channel R4 sampling, then an actual atomic run owner with
reusable stores/buffers, independent selected closure admission and versioned
authoring export. Installable reference releases should use these generic inputs,
not introduce a native kernel-specific encoder into Kagami or a worker. Remaining
management/adapters, security and attribution work stay in the full active goal.

Verification: all 56 shared plugin tests and 28 runtime tests pass, including the
real rebuilt Components, configuration source/default capture, malformed bounded
CBOR, quantity/type/constraint refusal, grid overflow/spacing checks and incompatible
boundary/grid rejection. Host all-target Clippy, standalone Wasm build/Clippy,
plugin/runtime doctests, workspace/reference formatting, documentation checks
(178 Markdown files) and `git diff --check` pass. No full-workspace application,
worker admission or graphical end-to-end result is claimed.

## Generic requested-channel sampling — 2026-09-16

Implemented shared R4 point-query/flat-response packets with exact observable
scientific identities, shape/dimension/frame/axis conventions, request-local point
IDs, source/state/context binding, compute precision and independent per-cell
validity/quality. Supported-but-invalid channels and unavailable exact contracts
remain distinct; invalid cells expose no numeric values. Request digests bind all
points, channels, source and context. Checked ranges/count products and explicit
point/channel/byte/value limits precede bulk allocation. Reusable output storage
has no heap allocation per cell; cold metadata/layout decoding still allocates.

Instance contexts now carry exact observable bindings and declared compute
precision. Bound host sampling verifies the actual retained state bytes, request
identity/schema/count, exact output extent and the guest's complete returned packet.
Missing/duplicate/invalid writes poison candidate completion. Sampling uses an
isolated disposable guest, so rejection cannot mutate retained scientific buffers.

Removed Newtonian's private `OGQ1`/`OGS1` bootstrap formats and rebuilt both actual
Components. Newtonian consumes arbitrary requested subsets/order of acceleration,
potential and Jacobian schemas, explicitly marks unsupported exact channels, and
preserves optional Jacobian invalidity without poisoning finite channels/forces.
No native Newtonian encoder or equation was added to the runtime/Kagami.

These references do not establish authored/committed snapshot provenance. The run
owner must supply leases and source artifacts; the committed run-descriptor content
reference must be tied explicitly to the existing protocol `RunIdentity`
(formation/workload/epoch), not become a competing run identity or mutable route.
Observer retention/cache/UI/MCP adoption remains work. Current numerical fixtures
still use synthetic provider pins, not independently admitted installable releases.
No application or concurrently edited MCP source was changed in this checkpoint.

Next: atomic fixed-profile scientific run ownership and reusable stores/candidate
buffers, then selected closure admission and captured authoring/workload export.
Keep aggregate/JIT budgets, failure attribution and outstanding management/product
adapters in the full active goal; this sampling checkpoint does not close X-PLUGIN.

Verification: 62 shared-plugin tests and 30 runtime tests pass, including the real
rebuilt components, requested subsets/order/unavailability, state/context/schema/
extent substitutions, malformed/truncated packets, flat ranges, buffer reuse,
invalid numeric-slot isolation and poisoned partial writes. Host all-target Clippy,
standalone Wasm build/Clippy, both crates' doctests, workspace/reference formatting,
documentation checks (178 Markdown files) and `git diff --check` pass. The initial
new JSON-value fixture adapter failed on borrowed digest decoding; using the real
string reader fixed the fixture. Clippy's test-style finding was corrected before
the final pass. No full-workspace application/worker or graphical result is claimed.

## Atomic fixed-profile run owner — 2026-09-16

Implemented `FixedRun` in the existing shared runtime, with no host-owned physics.
It validates captured field/history state without initialization, derives all field
and Dynamics projections from one whole-object packet, computes every field, reduces
complete force batches in stable order, integrates once and validates every candidate
before replacing all committed fields/objects/history/time together. Initial numeric
state includes static/kinematic and uncoupled objects; an integrator cannot change
membership, inertial mass or static objects through its output. Computed forces
retain their predecessor-boundary phase convention; initial state has no fabricated
force observations. Stop is explicit and terminal for that owner.

Added complete in-memory portable checkpoint/restore with exact run/program
compatibility and fresh-engine continuation evidence. Added bounded detached
`FieldSnapshot` leases with cached state identities, exact committed source checks,
one active query per lease and isolated guest sampling. The run's commit path never
acquires an observer budget lock. Exhaustion, stale queries and cancellation affect
the observer rather than scientific commit. Default lease policy is 8 / 128 MiB.

This is real atomic library execution, not worker admission. `RunProgram` is a raw
in-memory projection, not another workload manifest or proof of the selected release
closure. The source descriptor still needs explicit protocol `RunIdentity` linkage
at admission. `RunCheckpoint` is not a newly invented durable file format. The
reference tests still use synthetic contribution pins; no application/MCP source
was edited. See [the owner contract and limitations](../runtime-fixed-run.md).

Remaining full-goal work: reusable stores/arenas and aggregate/JIT budgets, variable
state sizing, runtime membership/emitter adoption, selected workload closure and
captured authoring export, durable observation/checkpoint/worker adapters, plus the
outstanding plugin-management authoring/UI/MCP/startup and hardening work. Do not
advertise the fixed-membership/fixed-extent owner as supporting those unimplemented
paths. The scientific packet budget does not include every temporary typed vector,
guest allocation or external checkpoint holder; full memory admission stays open.

Verification: 63 shared-plugin tests and 37 runtime tests pass, including actual
multi-field/Euler commits, no-field/empty-set behavior, failures in later fields,
Dynamics and final validation, exact fresh-engine continuation, concurrent old-state
sampling and observer quota refusal. Object-packet golden/hostile tests and the
production merge helper reject static/mass/membership rewrites. Host all-target
Clippy and both crates' doctests pass. A dead-code warning for a shared test helper
unused in the new integration binary was explicitly scoped and rechecked. Full
workspace application/worker/graphics validation has not been claimed.

The final pass also exercises Dynamics with a zero coupling budget when no fields
are selected; role-specific limits no longer confuse that with an object limit.
Both reference Components were rebuilt after the shared object-packet addition;
development artifact digests changed, without changing their portable state formats
or any published workload/document version. Standalone Wasm build/Clippy,
workspace/reference formatting, documentation checks (179 Markdown files) and
`git diff --check` pass.

## Selected scientific closure — 2026-09-16

Implemented `orishu_plugin::selected::{compile, verify}` with the versioned
`orishu.plugin-selection/v1` descriptor. Exact roots, complete transitive members,
requirement bindings, kernel instance uses and canonical release-root evidence are
identity-bearing. Compilation reuses the independent verifier; prior inventory
resolution and verified release handles do not grant admission authority.

The verifier checks selected membership, both ends of exact scientific bindings,
local-provider pins, field/coupling/Dynamics/observable roles, required family
channels, cycles/reachability and one-contract-per-selected-artifact. It bounds
descriptor structure, metadata and aggregate required bytes, then verifies only
the required root/payload/code closure. Unknown unselected payloads, unused kernels,
icons and documentation are not fetched or activated. Required blobs can be
verified from an exported set with no installation or inventory. See
[the descriptor and integration notes](../plugin-selected-closure.md).

Validation: all 74 plugin tests (11 new selected-closure scenarios), all 37 runtime
tests, both crates' doctests and all-target Clippy pass. Existing workload all-target
tests, including its canonical identity vectors and dependency boundary, pass.
Workspace formatting, `git diff --check` and documentation checks (180 Markdown
files) pass. No dependency, Wasm ABI, workload-v2 encoding or document-format change.
No full application/worker/graphics verification is claimed.

This is partial X-PLUGIN workload integration, not end-to-end export/admission.
The pure descriptor is not another workload root. Next bind it through an explicit
workload profile/version, cross-check graph instances and captured scientific
inputs/state, then perform actual ABI/configuration/state/policy admission into
`FixedRun`. Existing generic workload graph validation checks structure but does
not recognize a scientific execution profile; older-reader/fail-closed fixtures
remain part of that integration. Product management and the other outstanding
delivery gates above remain open. The full goal is still active, not blocked.

## Versioned composed root and captured-context binding — 2026-09-16

Added `orishu_workload::v3`: an explicit `orishu.dev/v3` workload resource retaining
the existing compute graph, metadata and execution requirements, with mandatory
selection/execution descriptors and remaining direct artifact descriptors. It
does not fabricate v2's required uniform spatial step/domain or global integration
selector. V2 bytes/types remain unchanged. Graph and streaming artifact validation
are reused, including the structural-before-retrieval gate. Required input roles,
typed canonical collection bounds and cross-version refusal are tested. Shared
descriptor accounting now rejects aggregate overflow even under a `u64::MAX`
policy instead of saturating to an apparently permitted value.

Added `ExecutionDefinition`/`CapturedField`/`CapturedKernel` portable input
descriptors: exact contexts, objects, couplings, field/history states and timestep.
The codec never initializes state. Added `selected::verify_context` to bind a
concrete use to selected artifact/scientific identity, format/profile, field state
bound, coupling properties/dimensions and complete observable vocabulary. Verified
release metadata is exposed for root graph provenance without inventory lookup.
See [v3's concrete format and current limits](../workload-v3.md).

This is still not complete scientific workload admission. Next assemble/check the
v3 graph, selected descriptor, captured inputs and exact required closure together,
then compile/validate the actual guests into `FixedRun`. The captured-definition
reader alone does not prove blob integrity, field-family uniqueness, correct
object projections, valid configuration/state, or kernel admissibility. Full
authored ECS/emitter/expression-provenance integration, variable output sizing,
aggregate/JIT budgets, worker delivery and management/product adapters remain open.

Verification: 76 plugin tests and 37 runtime tests pass, alongside all workload
tests including unchanged v2 identity vectors and five new v3 cases. The new v3
fixture digest is pinned after reviewing the explicit projection. All three crates'
doctests and all-target Clippy pass; formatting, documentation checks (181 Markdown
files) and `git diff --check` pass. No dependency or Wasm ABI change. This does not
claim full application/worker/graphics validation or that product endpoints accept
v3 yet. The full delivery goal remains active, with no external blocker.

## Fixed scientific-profile compilation and admission — 2026-09-16

Implemented `orishu_plugin::workload::{compile, verify}` for the explicit
`orishu.force-then-integrate/v1` v3 graph. Export retains only required selected
bytes and captured inputs, generates exact descriptor/evidence metadata, and uses
the independent verifier. Admission checks selected use coverage, exact graph and
closure equality, model-family exclusivity, common domain geometry, dimensioned
configuration, object/coupling consistency, role-property constraints and independent
budgets. Unknown profiles, unused selected vocabulary, missing/extra root artifacts
and ignored graph overrides fail closed. Reachability uses a bounded adjacency index;
coupling-property lookups are hoisted per slot rather than per entity.

Implemented `orishu_runtime::admit`: bounded canonical root decoding, required-resource
digest denial, independent profile verification, exact workload/run-scope agreement,
pre-compilation kernel count/code-byte budgets, actual Component compilation and
captured-state validation into `FixedRun`. Unsupported additional execution/hardware
requirements fail closed. No plugin installation, provider resolution or scientific
initialization occurs at admission. The embedding authority still supplies fenced
run identity; the library is not a cluster singleton guard or endpoint.

Added real reference release declarations and five admission tests. They export
independent vocabulary/solver providers and run actual Newtonian/Euler Components
in a fresh runtime with no inventory; unused executable bytes are excluded. The
integrator-only case excludes Newtonian code and drifts correctly. Tests reject
graph changes, absent/extra artifacts, denied code, stale coupling kinematics,
Dynamics flags, invalid slots/roles/counts/formats, over-budget inputs and malformed
opaque state with valid content hashes. The last case proves guest validation,
not reinitialization, decides state admissibility.

Validation: 76 plugin tests, 42 runtime tests and all 205 workload tests pass,
including unchanged v2 identity vectors. All three crates' doctests and all-target
Clippy pass. Workspace formatting, documentation checks (181 Markdown files) and
`git diff --check` pass. Initial Clippy map-entry/duplicate-test-module findings
were fixed and rechecked. No dependency, guest ABI or published v2 identity change;
the reference Wasm binaries were not changed by this checkpoint. No full-workspace,
worker endpoint or graphical application verification is claimed.

Still open: generic opaque initialization/output sizing (fixture-known extents are
not a product solution), full authored ECS/emitter/expression provenance, reusable
stores/arenas and aggregate/JIT accounting, document-format/registry adoption and
export, durable artifact delivery/recording, worker run fencing/control plane and
management/UI/MCP parity. Worker source inspection confirms formation services but
no workload execution endpoint. No concurrent MCP/application implementation was
changed in this checkpoint. The full goal remains active, not complete or blocked.

## Bounded initialization and installable reference bundles — 2026-09-16

Added explicit bounded output grants alongside unchanged exact extents. The
additive development-WIT `output.is-exact` method distinguishes the policies.
The host charges full byte ceilings before execution, grows candidate storage
only for checked contiguous writes and captures actual bytes/counts on exactly-once
finish. Over-ceiling values, hidden/unwritten bytes and duplicate completion poison
the candidate. Empty output is supported. Exact packet completion remains strict.

`InitializeBounded` and `InitializeHistoryBounded` now capture plugin-defined
natural state without caller knowledge of field/history sizing formulas. Bound
execution verifies exact context/schema and input identities before invoking guests.
The admission fixture uses generic ceilings for both actual reference kernels;
different sufficient capacities produce identical Newtonian state. Low-level
exact-layout fixtures remain intentional ABI/state-format tests. Advance/checkpoint
extents and membership in `FixedRun` are still fixed; adaptive state is not claimed.

Rebuilt both independent reference Components against the additive import:
Euler `90b9087dda07de5aba9eb913364cda5122686bcad7befc69494441e23b11abfa`,
Newtonian `33173f1e9230b109a6e7714683cba671ef622736c7c95d8702161029e34d5a49`.
Portable state formats and v2 workload identities are unchanged. Existing lifecycle
Components without the new import still pass exact-grant execution tests. A host
without the new imported method cannot instantiate a guest requiring it.

Added `package_reference`, a developer example using public declarations/identity/
bundle APIs and actual Component ABI checks. It produces separate vocabulary and
solver `.okplugin` files without installation or scientific execution and refuses
overwriting existing files. The shared reference release builder is also used by
admission tests. A real rebuilt Kagami binary installed solvers before vocabulary,
listed both exact releases, and disabled/re-enabled solvers in an isolated temporary
inventory. Those CLI calls each succeeded with versioned outcomes and revisions
0 through 4; no user's normal inventory was modified. See
[reproducible packaging/install commands](../../plugins/reference/README.md).

Validation: all 45 runtime tests pass, including the new bounded/hostile grant and
capacity-independent actual-guest tests. Admission was rerun after consolidating
the release builder; all six tests pass. All ten Kagami plugin-management tests,
runtime doctests/all-target Clippy, standalone reference Wasm build/Clippy, workspace
and reference formatting, docs checks (181 Markdown files) and `git diff --check`
pass. A concurrent cold-JIT admission exceeded the old outer test deadline; the
isolated rerun passed and only the outer test allowance was increased to 60 seconds.
Production guest operation deadlines are unchanged; the full suite then passed.

This is progress, not complete X-PLUGIN delivery. Exact-provider document adoption,
captured-state authoring and export, worker artifact/run endpoints, full composition/
emitter lifecycle, management UI/MCP parity and runtime hardening remain required.
No concurrent MCP application source was edited. No full-workspace or graphical
application acceptance is claimed. The active goal is not blocked.

## Exact component pins and verified authoring schemas — 2026-09-16

`ComponentTypeId` now distinguishes historical logical references from exact
provider-qualified `ContributionRef`s. Other extension points and hybrid spellings
are refused. Template-local aliases are independent of identity, preserved through
materialization and standalone expression evaluation. Same-named contributions
can coexist with explicit aliases; the same exact type cannot be attached twice
under different aliases. Hyphens in plugin local IDs map injectively to underscores
for expression segments; original scientific IDs stay in the declaration.

Catalog v2 accepts pins/aliases; unchanged legacy templates still write v1 with
the same fingerprints. Experiment JSON v3 accepts exact component pins; explicit
v1/v2 readers preserve logical references and upgrade only the envelope. Pins
mislabeled as old versions are refused. Model wire snapshots/commands advance to
version 2; session envelope shape remains version 1 with the model-version pairing
documented. Default-view versions, workload v2 identities and guest ABI are unchanged.
Digest serde now supports scratch-backed JSON/YAML and owned values without an
oversized diagnostic allocation. Tests retain old file fixtures rather than blessing
them as newly pinned content.

`ComponentSchema::from_plugin` projects verified release payloads with exact IDs,
full scientific role/bindings/requirements, retained defaults and scalar/text
constraints. The shared borrowed property predicate governs both authoring and
independent workload validation. Document commands, hydration and capability checks
enforce it, as do catalog validation and materialization after parameter overrides.
Defaults remain authored suggestions: missing values are not silently synthesized.
Catalog capture now recognizes shared unit symbols instead of treating them as
missing dependencies, and checks derived dimensions before projecting magnitudes.
Regressions cover `2000 g` as mass and refuse `2 m` as mass, including overrides.

`PluginStore::resolve_authoring` supplies the selection outcome and selected component
schemas from one verified inventory snapshot without a second read or execution.
Missing/disabled/ambiguous/stale selections supply no substitute registry. Tests
install a real declaration bundle, feed its resolved registry into the actual
document authority, reject out-of-range/overlong/omitted values atomically, round-trip
the saved experiment, and preserve its pins/state when the default release changes
or a process-only disable makes them unavailable. This is an app-owned adapter,
not yet application startup, plugin-selection UI/MCP or open-document lease wiring.

The catalog's shared-contract dependency adds no engine or transport. Audited
dependency ceilings are now catalog 36, document 37 and session 38; explicit
`orishu-runtime`/`wasmtime` exclusions supplement the existing IO/transport guards.
Exact-reference boxing keeps command rejection size inside its existing bound.

Both reference Components were rebuilt after the shared predicate/digest changes:
Euler `6d5c5c6e7052c7309809d1101c9798b5cc75b02215fc824512ee3d9b782a99d4`,
Newtonian `4ab1ee4d13fc95ae6fa17a00be25f0ab69b2f3efd7f2af3da5801f502cb2ecc1`.
These are development artifact identity changes, not scientific state-format or
WIT changes.

Verification: catalog, document, session, plugin and workload library/integration
tests pass, including unchanged workload-v2 identity fixtures. All 45 runtime
tests pass with the rebuilt Components. Kagami's complete library/integration
test set passes (including 12 management tests and the concurrent MCP surface).
Affected-crate all-target Clippy, shared-crate doctests, reference Wasm build/Clippy,
workspace formatting, docs checks (181 Markdown files) and `git diff --check` pass.
One management concurrency test hit `Busy` when unnecessarily reopening a store
for final inspection; it now reads the committed index through its existing
handle, leaving the two-writer race and its Busy/stale assertions intact. The full
application test set passed after that correction. No full-workspace test suite
or manual graphics/worker endpoint acceptance is claimed, and no concurrent MCP
source was edited.

Still required: scientific selection/domain persistence and migration, captured
opaque state with revision-guarded initialization/reinitialization and undo/save,
the document/blob container, full ECS/emitter/expression provenance, application
startup/selection/export, worker delivery/run authority, management parity and
remaining runtime memory/IO hardening. No complete X-PLUGIN or end-to-end product
acceptance is claimed. The full goal remains active and is not blocked.

## Native startup and component authoring adoption — 2026-09-16

Unix startup now loads the real local plugin inventory before constructing the
window/document authority. `--plugin-directory` and `KAGAMI_PLUGIN_DIR` address the
same store as management commands. Repeatable `--enable-plugin` / `--disable-plugin`
apply only to this invocation. Unknown/conflicting overrides, corruption and IO/
budget failures are refused before window or MCP startup. Non-Unix secure inventory
IO remains unimplemented and explicit override requests are refused there.

Discovery reads each release once from one index revision and independently resolves
at most 256 component contributions. One dormant contribution is reported without
hiding other usable vocabulary. Enabled non-default release schemas remain available
to existing exact document pins; no field model or integrator is selected by discovery.
Schema snapshots feed the real app model before opening a file or projecting MCP
session state. Historical demo schemas retain their distinct legacy identities.
The inspector's Add action submits the selected plugin's declared defaults as a
normal authoring command, retaining expression source and validating through the
authority. Missing defaults are not invented. The inspector explains this behavior.

Tests exercise installed vocabulary through the actual Add/undo/redo app-message
path, save to a real file, then reopen after a newer default release is installed.
The old pin and its values survive; disabling leaves the object present but
unavailable. Separate tests cover contribution-level dormancy, persistent-revision
stability and real-binary override refusal before window/MCP startup. Source/CLI,
authoring and main-parser tests pass; the existing MCP socket tests initially could
not bind under sandbox restrictions, then all eight passed with loopback permission.
The offscreen smoke test passed 120 frames under both projections using llvmpipe;
this is not a manual window/inspector visual check. No MCP module was edited.
All 15 management/startup tests, the authoring and parser tests, app all-target
Clippy, workspace all-target compilation, formatting and documentation checks pass.

No persisted, workload or Wasm ABI change in this checkpoint. Runtime execution,
field/domain/selection capture, document blobs, dependency-choice dialogs, live
inventory reload, open-document leases and remaining management parity/hardening
are still required. The full goal remains active, not complete or blocked.

## Installed selection to local initialization — 2026-09-16

The next document integration step exposed a missing production bridge: instance
contexts were hand-assembled in runtime fixtures. `selected::build_context` now
derives exact coupling roles/dimensions, channel vocabulary, model/format and
profile from verified selection. Input identities, precision, host bounds and
sampling quality allowances remain explicit. Its output passes the independent
context verifier; no provider name or reference-kernel convention supplies physics.

`PluginStore::prepare_selection` freezes a revision-checked installed selection,
loads only its selected artifact closure for retention/export, and acquires real
release leases before releasing the inventory lock. Missing/disabled dependencies
and ambiguity preserve the resolver's structured outcomes. No guest or JIT runs
under the inventory lock. Whole installed releases are still verified by the
existing bounded inventory reader; unrelated package artifacts are not retained
in the returned selected closure. Unrelated corruption therefore still fails
inventory preparation closed, consistently with startup discovery.

`apps/kagami/src/scientific.rs` adds a local initializer over the actual shared
worker sandbox. It resolves authored configuration/defaults with shared variables,
validates an explicit geometric domain, constructs the selected context and invokes
the real field or Dynamics lifecycle. Field initialization cannot receive objects.
Integrator history captures its exact initial Dynamics projection. Candidates own
immutable shared state/configuration/domain/context bytes with pinned format,
kernel and provider identities; output size is kernel-chosen within host ceilings.
Cancellation and scientific refusals remain structured, without arbitrary trap text.

Four app-level tests exercise installed independent vocabulary/model bundles,
exclusion of unused code, actual lease/removal behavior, stale/disabled/budget
refusal, real Newtonian/Euler initialization, declared defaults and coupling roles,
host-capacity-independent natural state, shared buffers, wrong execution roles,
cancelled work, dimension errors and unsupported boundary/discretization. A shared
builder test checks independent expected metadata and missing/extra/invalid channel
policy. The parallel management suite observed one transient `Busy` refusal; all
15 tests passed on serial rerun. This was not hidden by weakening lock assertions.

This is **candidate preparation**, not document acceptance or full-experiment
numerical validation. No field inspector/MCP command, persisted scientific state,
domain migration, export action or worker endpoint is claimed delivered. The next
step remains document-owned scientific selections/configuration and immutable blobs,
guarded async adoption/reinitialization, explicit legacy-domain reconciliation and
the versioned self-contained container. New runtime dependencies live in the app,
not the catalog/document/session cores. Persisted/workload/Wasm formats and reference
artifact identities are unchanged. No MCP module was edited in this checkpoint.

Validation: all shared-plugin tests and all 45 runtime tests passed; the four new
app initialization tests, 16 app-library tests, two CLI-parser tests and 29 authoring
tests passed. The management-suite serial rerun is qualified above. All-target
Clippy for the plugin/runtime/app, their doctests, nine catalog/document/session
dependency-boundary tests, workspace all-target compilation, formatting, diff and
documentation checks passed. Workspace compilation retains the pre-existing
`proc-macro-error2` future-compatibility warning. No new window or MCP socket smoke
test was run in this checkpoint; this adds no window/MCP action.

## Atomic scientific document core — 2026-09-16

`kagami-document::Setup` now has mutually exclusive `Legacy` and `Scientific`
variants. Existing files retain their global grid/boundary intent; there is no
implicit periodic/grid-to-isolated/continuous migration. Scientific construction
checks the exact verified selected declarations, common domain and input identities,
state formats, one model per exact field family, at most one integrator, complete
configured-use coverage and caller-owned capture budgets before adoption.
Opaque state/history, captured inputs and declaration maps are immutable/shared.

`AdoptScientificSetup` passes through the ordinary document transition/session
command path: one revision and undo entry, stale-revision rejection, whole-batch
validation and captured-state restoration. Final authored configuration is resolved
against the document's variables and exact declaration; changed parameters cannot
commit an old capture. Integrator history's source packet must match final dynamic
objects, kinematics and inertial mass. Object creation and replacement initialized
history can commit together; an isolated edit leaving history stale cannot. Missing
installed schemas do not prevent typed hydration: retained exact vocabulary checks
the authored mass without inventing an installed provider or pricing the UI's
unavailable component. Timestep edits retain initialization bytes and still require
runtime numerical validation before execution. Coherence validation currently adds
an eager bounded pass, not an incremental-performance claim.

Model wire version **3** describes scientific setup using blob identities, never
inline state bytes; the session envelope remains version 1. `WireCommand::of` is
now optional for effect-only commands, matching the session's existing refusal to
encode decoded-file effects as ordinary intents. This is **not** a new file format.
The existing JSON v1–v3 readers retain their explicit behavior, and the JSON writer
refuses scientific setup before temporary creation or target replacement. The
accepted self-contained container is the next implementation step.

The real-kernel application test now covers adoption, stale guards, shared-byte
undo/redo, metadata round trips, declaration/blob/domain policy, caller expression
bounds, variable-dependent configuration refusal, stale history refusal, atomic
object/history replacement, offline typed hydration and a real file left byte-for-
byte untouched by unsupported scientific saving. The initializer provides a typed
document capture from its original authored inputs; tests do not fabricate the
Newtonian/Euler opaque state.

Still required before exposing this as a window/MCP/file workflow: serializable
document-identity/expected-revision effect guards; current scientific availability
checks; the stored-ZIP/blob codec and explicit persisted domain migration; and
aggregate retained-buffer accounting across current state, undo/redo, idempotence
receipts and pending initialization. Current limits bound each captured setup and
history depth, not a unified retained-memory budget. Export, worker endpoints and
the other full-goal gates remain open. No MCP module was changed in this checkpoint.

## Scientific document container and durable IO — 2026-09-16

The [v4 scientific container](../experiment-container-v4.md) now implements the
accepted stored-ZIP document/blob profile. The durable file store selects it for
scientific setup and retains legacy JSON v1–v3 behavior for legacy setup. The pure
codec and public `decode_document` path independently restore exact field/history,
configuration and initial Dynamics projection bytes without executing code or
consulting installed plugins. Repeated identities share one restored buffer.
Neither file opening nor saving translates legacy boundary/discretization intent.

`VerifiedDeclarations` is a deliberately weaker, separate witness: exact release
membership, scientific payloads, dependency graph and contexts are verified, but
kernel bytes need not be installed. It cannot satisfy full-selection compilation
or admission APIs. Scientific documents retain that evidence, not executable
artifacts or unrelated plugin contributions. The plugin archive framing was
extracted into one shared pure module; package closure and digest checks remain
independent from ZIP CRCs and its original interoperability corpus still applies.

Container admission bounds physical bytes/entries and metadata before typed DTO
construction; the streaming preflight refuses an excess collection entry before
deserializing it. Referenced byte totals are checked before opaque buffer copying.
Golden empty-draft root, actual Newtonian/Euler round trips, offline hydration,
code exclusion, missing/extra blobs, forged descriptors and caller-budget tests
exercise public codecs. Real filesystem evidence covers save/reopen, readable
scientific backup, CRC recovery and refusal of a newer primary despite a valid
older backup. Unsupported candidates leave the old file untouched. The reader
now bounds its opened handle against file growth; read-budget refusal cannot be
treated as a missing primary and silently recovered around.

This closes the codec/durable-persistence gate, **not** overall scientific authoring
delivery. Immediate next work is aggregate retained-memory admission (including
Open command replay receipts, not merely undo) and guarded async initialization.
Current per-container/setup/history bounds do not provide one unified memory
ceiling; repeated large Open receipts are a specific release-hardening gap. Field
creation/reinitialization controls, current scientific availability, open-document
release leases, full authored export and worker endpoint/run integration remain.
Do not mark X-PLUGIN complete or ready for untrusted production use on this basis.
No numerical kernel, WIT, workload identity or MCP command module changed here.

Validation: all-target shared-plugin/document/session tests passed, including
dependency-boundary checks, unchanged legacy fixtures, the three new container
tests and 18 durable-store tests. All 45 runtime tests passed. App library (16),
CLI parser (2), authoring (29), plugin management (15, serial) and real scientific
initialization/persistence (5) tests passed. Scoped all-target Clippy, shared
doctests, workspace all-target compilation, formatting, diff and documentation
checks passed. Workspace compilation retains the existing `proc-macro-error2`
future-compatibility warning. Full workspace tests, a new window smoke test and
MCP socket tests were not run; no new scientific UI/MCP action was added.

## Authority scientific retention admission — 2026-09-16

The authority now enforces `ScientificLimits::retained_bytes` (default 512 MiB)
across current scientific state, both undo/redo stacks, and accepted command replay
receipts. The tally charges actual shared byte-buffer allocations once and cached
canonical metadata weights once per capture ownership group. Equal hashes in
separate allocations still count twice; allocation identity is transient accounting,
never scientific identity or persisted data. Strong pins prevent address reuse
during a tally, and a refused tally cannot be reused to bypass its ceiling.

Capture and scientific Open use a private staged authority transaction. Domain
validation, normal history policy, events and replay updates are published only
after retention admission succeeds. The oldest replay prefix may be evicted under
byte pressure, but the new receipt must fit and remain replayable. Undo/redo are
not silently trimmed merely to make a capture fit; refusal leaves the previous
revision, counters, dirty state, history, receipts and gesture/events intact.
Scientific preflight uses the same admission rule. Incoming batch counts and stale
guards are checked before cold transaction metadata copying. Open also checks the
receiving authority's per-setup scientific policy. Ordinary gestures/timestep
edits do not copy the authority and share captured buffers as before.

Tests include 300 independently decoded scientific Opens under a one-document
ceiling; newest-receipt replay; explicit lifecycle release; shared timestep/undo/
redo retention; equal-content independent allocations; real Newtonian/Euler
capture refusal at an exact boundary; preflight/submit agreement; unchanged
gesture/event/history/replay state on refusal; an intermediate capture retained
only by a batch receipt; and a tighter receiver's Open rejection. No persisted,
workload or WIT format changed; metadata weights are derived caches only.

This closes **authority-owned scientific retention**, not a total-process memory
budget. The cold transaction copies bounded schema/receipt metadata but no opaque
state; the ceiling measures byte buffers plus canonical metadata weight, not Rust
allocator overhead or unrelated object graphs. Shell pending initialization,
cancelled-but-still-running jobs, external snapshots and save/decode staging still
need reservations. Those and serializable document-identity/revision/intent guards
are the next integration gate, before field creation/reinitialization controls.
Workload export, worker execution endpoints and the other full-goal items remain.

Validation: shared-plugin/document/session all-target tests and their doctests
passed, including the new retention tests and existing dependency/golden checks.
All five actual-kernel initialization/document tests passed with the new retention
assertions; app library (16), CLI parser (2) and authoring (29) tests passed.
Scoped all-target Clippy, workspace all-target compilation, formatting, diff and
documentation checks passed. The workspace retains its existing
`proc-macro-error2` future-compatibility warning. The full runtime numerical suite,
full workspace tests, window smoke and MCP socket tests were not rerun in this
checkpoint; runtime equations/ABI and MCP modules were not changed.

## Document numerical projection to actual runtime — 2026-09-16

Implemented `kagami_document::projection::project` and the app-local
`workload::compile_numeric` seam. One immutable scientific snapshot supplies exact
role-bound object/Dynamics/coupling packets, captured configuration/domain and
opaque field/history bytes. Retained verified declarations resolve authored units
and expressions even after offline reopening left cached properties unresolved.
Component identity and schema version must match; unsupported components or
properties, legacy setup and missing integrators refuse instead of being dropped.
Object, field, coupling and packet/input-use bounds precede numerical allocation;
opaque captures are shared, never copied or initialized. The prepared installed
selection must equal the document's exact descriptor. The shared compiler still
independently validates the complete selected closure, and only required artifact
bytes appear in the resulting numerical workload.

The actual-kernel test now reopens persisted v4 data, adds both dynamic and static
gravity-coupled objects through document commands, checks SI source/response roles,
then compiles/admit/advances real Newtonian/Euler Components in a fresh runtime.
Static positions remain fixed while the dynamic object accelerates. Checks include
unchanged captured bytes, no unused code, unchanged authoring snapshot, tighter
projection budgets, mismatched kernel-instance selection and refusal of unmodeled
component intent.

This is deliberately **not full experiment export**: the bridge retains the source
revision in memory, but full authored ECS/expression/template provenance and emitter
definitions still need an explicit workload-profile artifact and independent
verification. There is no user export command yet. Close those gates before
advertising a complete portable experiment export; do not reinterpret the numeric
packets as the whole document. Worker endpoints/run control, guarded initialization,
pending-effect/IO reservations and all other full-goal items remain active.
No persisted format, workload identity, numerical formula, WIT or MCP module changed.

Validation: document/session all-target tests and doctests passed; all five app
scientific initialization/persistence tests passed with the new projection/runtime
assertions. The six real runtime admission tests, app library (16), CLI parser (2)
and authoring (29) tests passed. Scoped all-target Clippy, workspace all-target
compilation, formatting, diff and documentation checks passed. Existing
`proc-macro-error2` future-compatibility warning remains. Full workspace tests,
the remaining runtime suites, window smoke and MCP socket tests were not rerun;
there are no UI or MCP implementation changes in this checkpoint.

## Shared scene composition and source evidence — 2026-09-16

The previous numerical-only bridge now emits a shared `SceneDefinition` containing
the complete initial object/component/property composition, geometry, retained
quantity sources/units, variable evidence, explicit kernel configuration authorship
and location-free template fingerprints. `workload::compile_captured` replaces the
internal `compile_numeric` name; there was no public wire/CLI method to migrate.
The resulting closure retains additive data from independent plugins instead of
discarding everything not used in force/mass packets. Template names/paths are not
exported; fingerprints are evidence, never required artifact edges.

This uses explicit **execution descriptor v2** inside workload root v3. It requires
a digest-addressed scene-v1 artifact. Numeric-only execution-v1, workload-v2/v3 root
encodings, plugin identities and WIT are unchanged; scene-bearing workloads have
new identities and must be refused by older readers. The accepted contract and
[concrete protocol](../workload-v3.md#scene-bearing-execution-v2) document the shape,
limits and provenance semantics. Source expressions are not evaluated by workers.

Independent admission checks selected component membership, property completeness,
dimensions/constraints, exact object coverage, Dynamics compatibility, numerical
kinematics and complete field-coupling record/value agreement. Roots may now also
be reached from actual attached scene components; unused selections still refuse.
Nonzero angular motion on dynamic objects is explicitly outside this translational
profile, not silently ignored. Raw encoding bounds strings/structure before tree
copying; decoding bounds bytes/tree depth/work; projection preflights its own cold
copy budget and remaining total input allowance before building the scene.

Evidence includes a fixed canonical empty-scene vector; raw/wire bounds, truncated
inputs, duplicate/order/shape/source errors; unchanged numeric-only compatibility;
v4 offline reopen to scene-bearing real Newtonian/Euler execution; independent
data-only plugin values; eight component/projection tampering cases; and historical
source evidence that changes workload identity without becoming an evaluation or
fetch request. No document mutation, initialization or catalog lookup occurs during
compilation/admission.

**Still open:** portable workload packaging/public export commands, worker
delivery/admission/run endpoints, emitter blueprints and dynamic membership,
guarded initialization/UI/MCP management, pending-operation/IO reservations and
the other full-goal hardening gates above. This closes the fixed-membership
authored-composition/provenance gap, not X-PLUGIN as a whole.

Validation: shared plugin/document/session all-target suites and doctests passed,
including the new canonical scene vector, malformed/limit cases and descriptor
version separation. All 45 runtime tests passed (actual Component admission,
lifecycle, forces, fixed runs, numerical references, sampling and restart). The
five app initialization/persistence tests passed with scene compilation, tampering,
independent data-plugin and unknown-unit refusal checks; app library (16), parser
(2) and authoring (29) tests passed. Scoped all-target Clippy, workspace all-target
compilation, formatting, diff and docs checks passed. The existing
`proc-macro-error2` future-compatibility warning remains. Full workspace tests,
window smoke and MCP socket tests were not run; no MCP module/UI behavior changed.

## Portable workload bundle and headless export — 2026-09-16

Unix `kagami export <saved-experiment> --output <new-file> --name <workload-name>`
now freezes the document's exact installed selection, compiles captured scene and
scientific inputs, independently verifies the closure and publishes a new complete
workload file. Inventory guards and process-only overrides reuse the existing
authority. Stale or unavailable selections return bounded structured resolver
outcomes, not guesses or automatic retries. The command does not initialize,
execute guests, modify/recover the input, open a window/MCP listener, change
persisted plugin enablement or submit a worker run. Non-Unix IO fails closed.

`orishu_plugin::workload::bundle` supplies the shared pure pack/read boundary.
It reuses strict stored-ZIP framing with the distinct `workload.cbor` root and
exact required digest-addressed blobs. Extra cache bytes never enter output;
extra/missing physical entries refuse on read. No inventory or external fetch
participates in independent verification. Existing plugin/document container
encodings, workload root identities, scientific schemas and WIT are unchanged.

The [format/CLI document](../workload-bundle-v1.md) records bounds, error and
publication behavior, the buffered-API limitation and a reproducible framing-only
comparison against an OCI-shaped wrapper. Actual reference export: 1,328,386
bytes; equivalent OCI-shaped stored-ZIP framing: 1,328,974 bytes. The 588-byte
difference is not a performance justification: reuse of the bounded codec and one
unambiguous exact closure motivate the reversible choice. ADR 0010, contract,
architecture, roadmap and task summaries now distinguish this implemented path
from thin transfer and the complete researcher-facing workflow.

Evidence invokes the actual Kagami binary on a saved two-object captured experiment,
reads its output and admits/advances it in a fresh Newtonian/Euler sandbox after
dropping the export inventory. Source/pins/captures remain unchanged. Additional
cases cover deterministic bytes, unused-code exclusion, wrong container roots,
missing/extra blobs, corrupt/truncated archives, limits, stale/disabled selection,
duplicate/unknown overrides, temporary enablement without inventory mutation,
new-only publication, symlinks and refusal to recover a malformed input from backup.

Validation: plugin and session all-target suites and doctests passed; all five
app scientific tests (including actual CLI-to-runtime assertions), 15 plugin
management tests, 16 app library tests, three CLI parser tests and 29 authoring
tests passed. Scoped all-target Clippy and workspace all-target compilation passed.
Formatting, diff and documentation checks passed. Existing `proc-macro-error2`
future-compatibility warning remains. Full workspace tests, the standalone runtime
suite, window smoke and MCP socket tests were not rerun in this checkpoint; no
MCP module or numerical kernel changed.

**Still open:** guarded scientific creation/reinitialization and dependency UI,
pending-effect/IO reservations, window export, worker delivery/cache/admission/run
endpoints, emitter/dynamic membership, management parity and the remaining full-goal
hardening gates. The shared reader buffers complete archives; it does not implement
thin/ranged/streaming transfer or worker storage. X-PLUGIN remains active.

## Worker formation execution reservation — 2026-09-16

The existing worker formation owner now issues one `ExecutionLease` for pending
scientific admission. Eligibility is checked inside its serialized control lane:
the formation must be standalone, single-member and explicitly membership-locked,
with no reserved/outbound join (including pre-handshake IO). Two admissions cannot
both pass a stale published readiness read. The lease binds formation/node,
process generation and a checked non-reused sequence; it is not a scientific run
identity. New joins, unlock and leave refuse while the slot is occupied, without
breaking exact identified join receipt replay/conflict handling.

Shutdown revokes before acknowledging stopping. Formation replacement, ejection,
trusted internal topology changes and owner drop/failure also revoke. Revocation
does not free the slot while off-owner work still retains its lease. Lease drop
invalidates cloned read-only fences and sends its exact release through reserved
control capacity; a full mailbox or abandoned reply receiver cannot strand it,
and a late release cannot release a newer use. No Wasm, artifact buffers or scheduler
entered membership state. The sans-IO membership crate is unchanged.

[ADR 0028](../adr/0028-fence-worker-scientific-admission-through-formation-owner.md)
records the problem, rejected read-check/separate-lock/in-owner-computation options,
the first-profile topology restriction and the remaining adoption boundary. Seven
tests exercise the actual serialized worker owner, including pressure, source
generation, pending-join, replacement and held-shutdown/abort races. Existing worker
formation behavior remains unchanged when no internal execution lease is held.

Validation: worker all-target tests passed (184 library, 43 binary, 39 client
integration tests; two existing ignored diagnostic/child fixtures), including
benchmark smoke targets. Worker all-target Clippy, all-feature/all-target compilation,
workspace all-target compilation, doctests, formatting, diff and docs checks passed.
The existing `proc-macro-error2` future-compatibility warning remains. Full workspace
tests, real scientific worker adoption and public workload wire tests were not run:
the latter paths are not implemented yet. No Kagami/MCP module, scientific kernel,
workload identity, persisted or wire format changed in this checkpoint.

**Next:** consume the lease in bounded off-owner admission/execution and return
the candidate for serialized adoption with run/epoch identity and a final fence
check. Connect interruption and scientific publication, then versioned delivery/
run APIs and actual exported-file-to-worker execution evidence. `is_current()`
alone is not an atomic publication protocol and does not interrupt guest/JIT work.
No production endpoint invokes this reservation yet; summary `workload: None` and
the withheld execution capability remain accurate. This closes a prerequisite,
not worker execution or the full X-PLUGIN goal. All earlier full-goal gates remain.

## Worker off-owner scientific admission handoff — 2026-09-16

`apps/orishu-worker/src/driver/scientific.rs` now consumes the formation execution
lease in actual portable-closure verification and shared runtime admission. It
reserves confirmation capacity before input/work, runs decoding/verification/JIT
and guest validation outside the formation owner, enforces explicit input/profile/
executable bounds and a frozen required-artifact deny set, then returns a candidate
for serialized owner confirmation. Only identity/control/reply enters that lane;
runtime objects and their destruction do not. No plugin installation, kernel
selection or captured-field reinitialization occurs in the worker adapter.

The shared runtime now supports one monotonic parent cancellation per operation,
preserved through tighter policy bounds. Worker lease revocation interrupts linked
guest work; cancelling a child does not revoke the parent or sibling operations.
Aborting the admission future cancels its child, while the blocking job retains the
exclusive lease until actual disposal. Native JIT cannot yet be interrupted. The
confirmation future structurally retains runtime-before-lease disposal order on
success, refusal, timeout and cancellation, preventing early slot reuse.

Five worker tests use real independently compiled Newtonian/Euler Components and a
shared-compiler-produced portable closure. They prove admission and explicit
lower-level step handoff, responsiveness while scientific work is held on a blocking
executor, independent malformed/oversized/denied/wrong-scope refusal, capacity
retention after caller abort, shutdown before/after validation, and reserved
confirmation under mailbox pressure with a final topology check. These are not an
HTTP load journey or proof of externally published worker steps. The test host
supplies a descriptor digest; production run allocation is deliberately not invented
from a process-local lease sequence. Shared runtime fixtures were factored into
test support without duplicating their scientific setup.

Validation: all runtime all-target tests passed, including actual guest parent
cancellation; worker library tests passed (189, plus two pre-existing ignored
diagnostic/child fixtures) and all 43 binary tests passed. The 39-test worker client
suite initially had one `BrokenPipe` in the existing rejected-request lock test;
the full client suite passed unchanged on rerun. The all-target command therefore
was not a clean full pass, and its later benchmark targets were not reached.
Worker/runtime all-target Clippy, worker all-feature/all-target compilation,
workspace all-target compilation, doctests, formatting, diff and documentation
checks passed. Existing `proc-macro-error2` future-compatibility warning remains.
An earlier build exhausted `/home`; removing only the verified regenerable 31 GB
Cargo incremental cache restored space, and the affected checks were rerun.
No full workspace tests, window smoke or MCP socket tests were run in this pass;
no Kagami/MCP module, numerical kernel, WIT or persisted/wire format changed.

**Still open:** actual long-lived worker execution ownership, real run-descriptor/
epoch allocation, atomic initial/step publication coordinated with topology,
bounded input IO/artifact delivery and versioned public workload/run APIs. Owner
confirmation is a validated handoff, not a published Loaded/Ready resource and not
permission for unfenced commits. Worker summary `workload: None` and withheld
execution capability remain accurate. Aggregate RSS/interruptible JIT and all
earlier authoring, management, emitters and full-goal hardening gates remain open.
ADR 0028, architecture/context, task status and roadmap now record this distinction;
the roadmap also no longer lists implemented headless captured-file export as
missing. X-PLUGIN remains active, not complete.

## Retained worker execution and fenced boundary publication — 2026-09-16

`ValidatedAdmission::start` now retains the admitted shared runtime and execution
lease on a blocking executor. `RunHandle` provides explicit fixed steps, terminal
stop, bounded immutable field acquisition and unload; metadata reads use the
formation owner's accepted `RunView`. Clones share one operation slot, held through
execution, with no waiting step/workload backlog. Dropping a reply future does not
undo an accepted command. Dropping all handles unloads after active work; explicit
unload additionally revokes the lifetime. Idle revocation is checked every 50 ms,
while active guest work uses the existing linked cancellation. Runtime destruction
still precedes lease release and happens outside the formation owner.

Shared `FixedRun::advance_with_commit` validates a whole candidate, performs the
final cancellation check and calls its trusted embedding commit gate exactly once.
Gate rejection preserves every prior buffer/time/boundary with typed publication
phase attribution. After acceptance no additional fallible/cancellation check can
roll back behind an external publication. Plain `advance` keeps its existing local
semantics through an always-accepting gate. No guest ABI or numerical code changed.

The worker reserves publication capacity before computation. Its gate sends only
small scope/boundary/time metadata to the formation owner, which rechecks exact
lease/generation, eligibility, run identity and boundary progression in the same
serialized lane as topology and shutdown. Initial state, each step and terminal
stop are now published internally. Actual scientific buffers and field-lease
requests remain with the executor, and observations are serviced only after
acceptance/private commit. Lost or rejected publication coordination terminates
the execution lifetime instead of allowing an uncertain private/public state pair
to continue. This is not the durable receipt/retry protocol for public APIs.

Three real-Component worker tests cover repeated Newtonian/Euler steps, exact
boundary/time, cancelled-step preservation, terminal stop, observer quota refusal
without blocking later steps, retained old snapshots and sampling after unload;
bounded operation refusal and reserved publication after caller disconnect; and
fully computed late candidates after actual owner shutdown/abort. A shared runtime
test additionally checks whole-state gate refusal and finality after acceptance.
Heavy worker scientific fixtures now share one test-only concurrency permit: the
first broad parallel run hit two 60-second admission deadlines during simultaneous
fresh JITs. Each fixture still creates and validates its own real engine/Components;
production deadlines, guest limits and coordination bounds were not relaxed.

Validation after that fixture correction: the full worker all-target command
passed (192 library tests, two pre-existing ignored diagnostic/child fixtures,
43 binary tests, 39 real client integration tests and benchmark smoke targets).
The full runtime all-target suite passed; the commit-gate test was additionally
rerun after adding publication-phase attribution. Worker/runtime all-target
Clippy, worker all-feature/all-target and workspace all-target compilation,
doctests, formatting, diff and docs checks passed. Existing `proc-macro-error2`
future-compatibility warning remains. Full workspace tests, window smoke and MCP
socket tests were not run. No Kagami/MCP module, kernel code, guest WIT or existing
persisted/wire format changed in this checkpoint.

**Remaining:** real immutable run-descriptor/epoch allocation, bounded portable
input delivery, daemon/public workload and run APIs, continuous control, durable
identified receipts, checkpoint/storage and observation wire integration. The
formation-v1 public summary and advertised capabilities are unchanged; no daemon
IO adapter starts this internal executor yet. All earlier guarded authoring,
pending-effect/IO, management parity, emitter/dynamic-membership and runtime
hardening obligations remain. Architecture, ADR 0028, runtime/task documentation
and roadmap now distinguish internal retained execution from public delivery.

## Owner-allocated run descriptors and real bootstrap — 2026-09-16

`orishu::model::run` now defines the existing protocol execution tuple and the
immutable `orishu.run-descriptor/v1` record. Its explicit deterministic CBOR uses
the shared workload codec, with a 512-byte input bound and fixed shape/depth;
SHA-256 binds formation, exact scientific workload root and workload epoch.
Independent golden bytes/digest, typed serde interoperability, maximal identities
and hostile encodings are tested. JSON formatting is not identity. The record
contains no routing, labels, credentials or mutable state and is not injected into
the workload closure. This adds a versioned run artifact, not a workload or guest
ABI migration. See [ADR 0029](../adr/0029-bind-run-descriptors-to-owner-allocated-epochs.md)
and [the descriptor specification](../run-descriptor-v1.md).

The serialized formation owner allocates epochs independently of reservation
sequences, only after portable closure and denial checks, before Component
admission. One lease/root keeps one descriptor; conflicting or stale requests
are refused. Failed admissions, dropped replies, release and unload never reuse
issued epochs; exhaustion refuses without wrapping. Allocation consumes capacity
reserved before admission. Confirmation and each boundary publication check the
exact allocated scope, not a caller-supplied run digest.

`RunningWorker::scientific_admission` now binds this service to real worker
bootstrap, and preparation requires an explicit expected formation. Actual
Newtonian/Euler Components execute after identified lock, admission, confirmation
and retained-run start. The restart test uses the same credential directory and
labels, proves a new formation/descriptor, rejects the prior formation, and steps
the real runtime. Repeat execution within one formation gets a new epoch while
old field leases retain their original provenance. Owner tests additionally cover
lost replies, full ingress, stale leases, forged scopes and epoch exhaustion.

Validation: the full `orishu` all-target suite passed (101 unit, seven existing
wire and three new descriptor tests). The full worker all-target suite passed
(196 library tests, two pre-existing ignored fixtures, 43 binary tests, 39 real
client tests and benchmark smoke targets). The new real bootstrap/restart test
also passed independently. Workload dependency-boundary tests, both affected
crates' doctests and all-target Clippy, workspace all-target compilation, worker
all-feature/all-target compilation, formatting, diff and docs checks passed.
The initial sandboxed Orishu suite could not bind its mock listeners; the complete
suite passed with local-listener permission. Existing `proc-macro-error2`
future-compatibility warning remains. Full workspace tests, GUI smoke, MCP socket
tests and the unchanged standalone runtime suite were not rerun in this checkpoint.

**Remaining:** this is still an internal daemon-adapter seam, not a public
workload delivery route or an advertised execution capability. Bounded input IO,
identified durable receipts, public control/retrieval, continuous execution,
distributed/reset allocation, storage and observation wire integration remain.
The full-goal authoring, management, emitter and runtime-hardening gates above
remain open. No Kagami/MCP module or numerical kernel changed in this checkpoint.

## Guarded window field reinitialization — 2026-09-16

The Unix window can now explicitly reset one captured field to its selected
kernel's natural initial state. The scene tree's **Scientific setup / fields**
entry opens the field inspector, whose Authoring-only button starts the same
local initializer used by the worker-compatible sandbox. No entity packet or
previous field state enters field init. Other fields and Dynamics history retain
their exact bytes. The candidate reaches `Document::edit_guarded` as one ordinary
`AdoptScientificSetup` command; undo/redo restores captures without running code.
Opening, saving and exporting remain non-initializing operations.

`Document` now correlates effects with a transient incarnation/context UUID and
the expected experiment revision. Different documents, New/Open, schema refresh,
intervening edits/undo, and entering then leaving observation cannot resurrect a
stale completion. Camera changes and saves do not invalidate scientific work.
The final submission still passes through the workspace gate and shared document
authority with an explicit expected revision. `kagami_document::variable_context`
reuses the existing dimension-aware document compiler and checks caller count/
source bounds rather than converting variables to unitless magnitudes.

`ScientificEffects` holds one non-queuing native background job, never JITs on the
window update thread, and polls only while pending. Cancellation or context loss
does not release capacity until actual thread exit; native JIT remains
non-interruptible. Pending scientific input is capped at 128 MiB before spawning,
selected artifact bytes at 64 MiB with an 8 MiB metadata ceiling, and new opaque
state at 16 MiB. Candidate retention is checked separately. These logical bounds
are not an aggregate RSS/JIT guarantee; existing sandbox limits still govern the
guest. Other shell IO/effect budgets are not implemented by this single lane.

Startup supplies its exact inventory revision and process-only overrides to the
effect. Selection pins never drift to new defaults. After guest execution, a
nonblocking shared inventory-revision guard prevents management mutations until
the completed candidate is adopted/discarded. No inventory lock is held across
JIT or guest execution. A mutation that wins first causes stale refusal; a later
writer gets Busy and must retry explicitly. The selected release leases span the
job. Full open-document leases and live inventory refresh remain future work.

Evidence: actual installed Newtonian/Euler Components through the window message
path, one accepted revision, preserved history, exact undo/redo, late completion
after document replacement, stale inventory refusal and lock-race tests. Separate
guard tests cover cross-document equal revisions, mode round trips, schema/edit/
undo invalidation, camera-only survival, duplicate completion and dimensioned
variables. A held native-thread test proves cancelled work retains the slot.

Validation: all 80 Kagami all-target tests passed, including eight existing MCP
wire tests and five real scientific tests (serialized to bound fresh-JIT fixture
contention). Document and session all-target suites, affected-crate doctests,
Kagami/document all-target Clippy, workspace all-target compilation, formatting,
diff and docs checks passed. The later scene-tree navigation/cancel presentation
refinement passed Clippy and all authoring/guard tests. `make smoke-kagami` passed
120 frames under both projections on software Vulkan/llvmpipe; it is an offscreen
renderer check, not manual verification of the inspector layout. No full workspace
test suite, distributed/worker regression rerun or manual window check was done.
No existing persisted format, guest ABI or numerical kernel changed. MCP server
implementation was left untouched; scientific MCP parity is not claimed.

**Still open:** field creation and domain/compute-parameter editing, multi-field
atomic initialization, Dynamics-history regeneration on entity edits, dependency
selection UI, scientific MCP adapters, inventory refresh and full lifetime leases,
remaining pending IO/effects, window export and the worker delivery/run path, plus
all earlier emitter and runtime-hardening obligations. X-PLUGIN remains active.

## Explicit window physics setup creation — 2026-09-16

The Unix **Scientific setup / fields** inspector now includes an explicit model,
domain/grid and timestep form, including for an initially empty experiment.
Startup reads at most 256 enabled field/integrator contributions under the same
inventory revision and process-only overrides as component vocabulary. The form
selects one integrator and up to 64 field models, with exact release-qualified
identities. It uses declared parameter defaults, binary64 and direct sampling;
unsupported defaults or missing/ambiguous dependencies refuse, never select a
different provider. Consent is required to replace physics/reset initial states
and is cleared when choices change. Form input lengths, finite/positive numbers,
domain/grid limits, roots and selection counts are bounded. Generated instance IDs
remain canonically ordered beyond ten uses.

`ScientificEffects::configure` prepares the whole candidate in the existing single
guarded background lane. All fields initialize without entities or previous state.
The integrator receives a bounded initial packet projected from the current
objects with its exact selected Dynamics component. Static objects are absent
from that packet; schema version, positive inertial mass, dimensioned expressions,
position and velocity are checked. `kagami_document::scientific::initial_dynamics`
and final captured-history validation share the same projection, without adding
runtime/IO dependencies to the document crate. Existing objects and their exact
component pins are preserved; unpinned legacy components require explicit migration.

No field/history capture is adopted until the entire candidate succeeds and the
existing document/mode/revision and inventory guards still hold. Adoption is one
ordinary undoable command. Failed later initialization leaves prior scientific
state intact. This lane retains the prior 128 MiB scientific-input cap and 64 MiB
selected-artifact/8 MiB metadata caps; new opaque outputs share a 128 MiB ceiling,
with per-kernel capacity capped at 16 MiB. The initial Dynamics packet has a 16 MiB
and 65,536-record ceiling, further restricted by document object limits. These are
logical retention limits, not aggregate native JIT/RSS guarantees.

Evidence: real installed Newtonian/Euler Components through window messages, an
existing dynamic object plus static object, exact history kinematics, one-revision
adoption, exact undo/redo and a later-kernel failure with no partial adoption.
A separate form-driven test creates physics from a new document, saves v4 and
exports/validates a self-contained portable workload. It checks process overrides,
reset consent and invalid input. Pure form tests cover changing the single
integrator, canonical IDs for twelve uses, malformed/oversized input, invalid grid
counts and nonfinite/nonpositive timesteps. This does not yet provide a real
multiple-field GUI fixture or scientific MCP parity.

Validation: Kagami's full all-target suite passed (82 tests, including seven real
scientific tests and eight MCP wire tests); the two subsequently added form unit
tests passed separately. Document/session all-target suites, affected doctests,
Kagami/document all-target Clippy, workspace all-target compilation and docs/diff
checks passed. Clippy initially caught test-module placement and passed after it
was corrected. `make smoke-kagami` passed 120 offscreen frames under both
projections on software Vulkan/llvmpipe. Manual inspector layout verification and
the full workspace/worker/runtime test suites were not rerun. No persisted format,
WIT or numerical kernel changed. MCP server code was left untouched. To recover
build capacity, only the 12 GiB regenerable Cargo incremental cache was removed.

**Still open:** custom parameter/dependency editors, initialization/history
regeneration for subsequent entity edits, scientific MCP adapters, live inventory
refresh/full lifetime leases, other pending-effect/IO reservations, window export,
public worker delivery/run control and all earlier emitter/runtime-hardening gates.
The form is a complete-setup proposal, not an editor populated from arbitrary
existing captured configuration. X-PLUGIN and the full delivery goal remain active.

## Schema-driven kernel parameters and captured-setting copy — 2026-09-16

The Unix complete-setup form now renders configuration controls from verified
field/integrator declarations, with no reference-kernel property names in UI
code. Quantity expressions retain their source and units, booleans are explicit
literals, and text is bounded by UTF-8 bytes. **Provide value** distinguishes an
override from default/omission; explicit false and empty text survive. Required
values without defaults are not fabricated. The existing shared configuration
compiler, initializer and document authority decide dimensions, constraints,
kernel compatibility and final acceptance; form edits alone create no revision.

Parameter overrides have a 4096-byte per-input ceiling, 1 MiB aggregate text
ceiling and 4096-entry ceiling, including retained overrides for deselected models.
Type/size/selection failures preserve prior input and surface a bounded local
error; Apply refuses until corrected. Removing/replacing accepted overrides
reclaims the corresponding budget. Model discovery checks its 256-choice limit
before cloning another declaration and bounds copied parameter metadata at 8 MiB
(property storage, identifier/default text, excluding allocator overhead). These
limits do not replace the inventory's existing IO or guest/JIT budgets. The effect
accepts up to the shared schema limit of 256 configuration properties per kernel,
with existing per-source/document/guest limits still enforced.

**Copy captured settings into form** is explicit and non-executing. It preserves
exact model/instance IDs, provider bindings, authored overrides, domain/grid,
timestep, precision and sampling policies. Missing exact models refuse; no default
substitution occurs. This mode locks model choices, while **Start new physics
proposal** explicitly drops local settings/pins without editing the experiment.
Reset consent is cleared when parameters change. Applying copied settings is
still a complete-setup replacement that explicitly resets **all** initial field
states and integrator history. Targeted per-field parameter edits preserving
unrelated state are not claimed implemented.

Evidence: pure form tests cover retained quantity source through shared unit
evaluation, boolean false versus default true, explicit empty text versus a
nonempty default, undeclared/unselected/wrong-type/oversized inputs, aggregate
byte/count ceilings and budget reclamation. The real installed Newtonian/Euler
window test now authors a radius in millimetres, copies exact captured settings,
refuses a kilogram-valued radius without changing the revision, then applies a
corrected value. It checks exact pins, instance IDs and sampling policy, one
accepted revision, exact undo/redo, save/reopen and portable export; reopening
retains both authored source and resolved SI value. No kernel, WIT, persisted
format or MCP server implementation changed in this checkpoint.

Validation: the full Kagami all-target suite passed (87 tests, including seven
real scientific tests and eight MCP wire tests). The subsequent finite-number
display refinement and all six form tests passed independently; maximal, tiny
and subnormal finite values retain exact bits within the form's text limit.
Kagami all-target Clippy, workspace all-target compilation, Kagami doctests,
formatting and docs/diff checks passed. `make smoke-kagami` passed 120 offscreen
frames under both projections on software Vulkan/llvmpipe. Manual parameter-form
layout verification remains outstanding. Full workspace/worker/runtime and
separate document/session suites were not rerun for these app-local changes.
The existing `proc-macro-error2` future-compatibility warning remains.

**Remaining:** targeted field-parameter effects, dependency-choice UI/MCP,
object-edit history regeneration, live inventory refresh and lifetime leases,
window export, management parity, public worker delivery/run control, and the
earlier emitter/distributed/runtime-hardening gates. X-PLUGIN remains active.

## Lease-bound worker workload body delivery — 2026-09-16

`PreparedAdmission::receive` now consumes a body-bounded async reader under the
existing exclusive formation execution reservation. A positive exact announced
length is checked against host/archive policy before allocation or input polling.
Default `DeliveryLimits` allow 128 MiB and an absolute 30 seconds, independent of
progress and further bounded by operation policy. Reads use at most 64 KiB per
quantum plus one byte to reject overrun, check cancellation/revocation during
stalls and yield between ready chunks. Truncated/excess bodies, IO failure and
deadline expiry are typed bounded refusals; EOF itself must arrive within budget.
Public adapters must supply end-of-body semantics, not read a reusable connection
or an arbitrary path to EOF.

The receiver uses one fallible reservation for its payload allocation without
zero-initializing unfilled bytes. It closes the reader before native verification
and moves the Vec into the existing off-owner job without a whole-buffer Arc
conversion/copy. This bounds logical input retention, not allocator overhead,
guest/JIT memory or total RSS. Aborting IO releases its lease; after native work
begins, cancellation retains the lease until actual job disposal. No second queue,
cache, filesystem staging or worker plugin installation was added.

The request's expected workload root is compared with the independently verified
complete closure **before epoch allocation or JIT**. Wrong-root delivery therefore
cannot consume a run epoch or silently execute a different valid workload. Correct
identity still passes existing deny-policy and scientific admission. Admission
remains a candidate until owner confirmation and initial-boundary publication;
delivery alone is not a command receipt or a loaded run.

Six focused tests cover pre-read length/policy refusal, truncation/excess/IO/invalid
closure, stalled-EOF exclusivity, cancellation/abort/shutdown, absolute deadlines
despite progress, bounded read quanta, and cancellation after delivery retaining
native-job capacity. A real Unix-socket body containing the shared reference
compiler's portable Newtonian/Euler closure reaches owner-confirmed retained
execution and a committed step; a prior wrong-root body leaves the first issued
epoch available. A follow-up assertion proves the reader was dropped before the
held native job begins. No new guest, workload, run-descriptor, formation or client
wire format changed. Kagami/MCP implementation was not edited in this checkpoint.

Validation: all six focused delivery tests passed. The full worker all-target
suite passed (202 library tests, two existing ignored fixtures, 43 binary tests,
39 real client/process tests and benchmark smoke targets). The subsequent
reader-drop assertion passed independently. Worker all-target Clippy, workspace
all-target compilation, worker all-feature/all-target compilation, worker
doctests, formatting and docs/diff checks passed. Existing `proc-macro-error2`
future-compatibility warning remains. Full workspace, standalone runtime and
Kagami test suites/GUI smoke were not rerun for this worker-only implementation.

**Next:** authenticated public request framing, identified durable command
receipts/retry, daemon ownership/retrieval/control and actual Kagami-to-worker
delivery. Public workload routes and execution capability stay disabled until
that path is verified. Thin/resumable transfer, cache administration, distributed
allocation, storage/observation wiring and all earlier full-goal gates remain open.

## Durable identified worker load-receipt foundation — 2026-09-16

The shared `orishu::model::run_load` now defines strict versioned load intent and
historical receipts. Intent binds the existing operation ID, expected formation
and immutable workload root, not archive bytes/length or plugin paths. Accepted
receipts validate the owner's descriptor against that request and a nonzero epoch
in constructors and serde. Pending intent, known acceptance, known refusal and
indeterminate outcomes remain explicitly distinct. No formation-v1, workload,
kernel WIT or run-descriptor format was changed.

The app-local Unix `workload_receipts::ReceiptStore` persists bounded history in
the worker's existing private directory. It shares the credential adapter's
private-directory/file checks without changing bootstrap identity behavior. A
separate single-writer lock, private fd-relative staging/rename, file sync and
directory sync precede issuing a completion ticket or final receipt. Tickets
cannot complete another store/incarnation. IO failure poisons the instance until
reopen; interrupted Pending records recover durably as Indeterminate, never as
new execution permission. Final history survives unload and changed source node.

The fixed first backend allows 256 receipts/512 KiB, uses the existing bounded
CBOR preflight and refuses an excess typed entry before deserializing it. Full
history still serves exact retry/conflict checks without evicting old identities.
Malformed, incompatible, duplicate, oversized or unsafe existing files fail
closed rather than reset history. The store itself does not own formation,
authorization, live runs or scientific persistence. It is not opened by daemon
bootstrap and no public route/capability uses it yet.

[ADR 0030](../adr/0030-retain-durable-worker-load-receipts.md) records the options,
conservative crash semantics and limits; [the fact/storage specification](../run-load-receipts-v1.md)
defines exact shapes and required coordinator integration. Architecture, context,
protocol entry points, worker README, tasks and roadmap distinguish implemented
internal foundations from public product delivery. Kagami/MCP code was not edited
in this checkpoint.

Validation: nine focused journal tests cover persistence-stage faults, restart,
at-capacity replay/conflicts, foreign/stale tickets, corrupted and unsafe paths,
exclusive locks, credential coexistence and refusal before excess-entry decode.
Two shared fact tests cover actual JSON/CBOR round trips, strict shape/version and
cross-identity acceptance rejection. The existing real Unix-socket Newtonian/Euler
test now persists Pending before delivery, records Accepted only after initial
publication, advances the run, unloads, reopens and replays unchanged acceptance.
The full worker all-target suite passed (210 library tests, two existing ignored,
43 binary tests, 39 real client/process tests and benchmark smoke); the subsequent
early-count test passed with all nine journal tests. The shared Orishu all-target
suite passed after rerunning with local-socket permission: the first sandboxed
attempt could not bind its mock HTTP servers. Worker/shared all-target Clippy and
workspace all-target compilation passed. Worker all-feature/all-target compilation,
shared/worker doctests, formatting, diff checks and documentation validation (188
Markdown files) passed. Existing ignored fixtures and the `proc-macro-error2`
future-compatibility warning remain; no new check failures remain. Full-workspace
tests, standalone runtime tests and Kagami tests/GUI smoke were not rerun for this
worker/shared-model checkpoint.

**Next:** implement the daemon-owned admission coordinator and authenticated public
framing/replay/retrieval using this journal plus the existing execution lease.
Retain tickets and accepted runs independently of HTTP futures, handle uncertain
publication/persistence without false refusals, and prove dropped-connection and
restart paths before enabling execution. Public run control/observations, actual
Kagami-to-worker delivery, distributed/reset allocation, durable scientific
storage, cache administration and earlier full-goal gates remain open. X-PLUGIN
remains active; this checkpoint is not end-to-end completion.

## Daemon-owned admission, receipt projection and retained runs — 2026-09-16

`workload_load::LoadCoordinator` now composes the durable journal, existing
formation-fenced admission and retained executor. New submissions claim one
non-queuing local job slot, persist intent on a blocking IO lane, then acquire the
owner's execution lease before polling a body/JIT. Tickets and accepted runs are
owned by the detached daemon job, not the response future. Exact replay/conflict
uses the journal's shared bounded read-only history projection, without waiting
behind IO or compilation. History is rechecked under the local admission gate to
avoid treating a just-completed operation as new work.

`RunningWorker::install_load_coordinator` installs and retains this owner once
from an already opened journal, sandbox and host policy. A distinct daemon-owner
guard shuts down even if client clones survive daemon drop. Ordinary response/
handle loss does not abort work; explicit shutdown cancels admission and revokes
retained-run clones. Exact formation/root/epoch is required for run retrieval,
and a historical acceptance cannot silently select a replacement execution.
The usable retained descriptor can also be discovered independently of receipt
health, so a client need not guess its epoch after receipt persistence failure.
Unload permits a new owner-allocated epoch after actual native lease disposal.

Known pre-start failures become durable bounded refusals. Start failure is
conservatively Indeterminate because its error surface includes lost publication
acknowledgement. A successful run is retained before final receipt IO; failing
that IO leaves the known run retrievable and the receipt projection poisoned,
never fabricates a refusal or grants new admission. Unexpected job failure also
poisons history/cancels the active control; reopening recovers its pending intent.
Job completion publishes idle under the same gate as dispatch, so an old job
cannot overwrite a newer job's busy state. Dispatch releases that gate before
spawning, allowing immediate executor-side future disposal without deadlock.

Seven coordinator tests exercise actual daemon retention after lost response/
client handles, exact replay without body reads, pending overload/cancellation,
stale formation/invalid input/length, unexpected task panic and restart, known
published execution despite final receipt IO failure, daemon drop with surviving
clients, install-once behavior and failed reservation/invalid host policy. Actual
Newtonian/Euler Components advance retained runs and prove fresh epochs after
unload; no fake guest or substitute runtime was added. Existing receipt, workload, run-descriptor and kernel wire
formats are unchanged. Kagami/MCP code was not edited in this checkpoint.

Validation: the full worker all-target suite passed (218 library tests, two
existing ignored fixtures, 43 binary tests, 39 real client/process tests and
benchmark smoke targets). The seven coordinator tests passed again after the
final lifecycle-gate fix and again with retained-descriptor discovery assertions.
Worker all-target Clippy, workspace
all-target compilation, worker all-feature/all-target compilation, worker
doctests, formatting/diff checks and documentation validation passed. Existing
`proc-macro-error2` future-compatibility warning remains. Full workspace tests,
standalone shared runtime tests and Kagami tests/GUI smoke were not rerun for this
worker-only checkpoint.

Architecture, ADR 0030, fact/coordinator documentation, worker README, task status
and roadmap now distinguish internal daemon integration from public delivery.
Main's serving path still does not automatically open/install the journal and
sandbox. **Next:** bounded authenticated HTTP framing, explicit serving-path
configuration and receipt/run retrieval/control adapters consuming this owner;
test real dropped connections and process restart before enabling execution.
Kagami-to-worker submission/observation, durable scientific storage, distributed/
reset allocation, cache administration and all earlier full-goal gates remain
open. X-PLUGIN remains active.

## Opt-in authenticated scientific HTTP admission — 2026-09-16

`--scientific.enabled true` (environment/file equivalents with explicit CLI >
environment > file precedence) now opens the private journal/shared sandbox and
installs the daemon coordinator before client listeners. Default startup remains
formation-only and does not create receipt history. The Unix durable backend is
required; malformed configuration and corrupt history fail closed.

The app-local serving adapter implements `POST /api/v1/run-loads`,
`POST /api/v1/run-loads/lookup` and `GET /api/v1/run`. All require exactly one
worker operator bearer credential, including local reads, before body parsing.
Eight scientific handlers are shared across listeners independently of membership
mutation capacity. The upload uses a versioned media type, a bounded u32/CBOR
intent prefix and one complete portable bundle with canonical Content-Length.
Metadata, archive length, EOF, transfer progress and total admission have explicit
host bounds; no arbitrary URL fetch, plugin installation or disk cache was added.

HTTP body consumption retains one data frame at a time, rejects trailers/errors
as EOF, and yields after a bounded empty-frame quantum. GET checks actual EOF,
including HTTP/2 without Content-Length. After complete upload EOF a Pending/202
receipt can return while daemon-owned admission continues; final receipt retrieval
uses 200 irrespective of Accepted/Refused/Indeterminate. Neither status is itself
scientific acceptance. Request/response lifetime never owns the admitted executor.
Historical lookup survives restart; current-run discovery is independent of
receipt health and never presents historical acceptance as a restored run.

Scientific occupancy now reports `schemaVersion: 2`, `workload: "scientific"`
from the same formation-owner view, instead of falsely claiming an empty worker.
The shared decoder accepts v1/None and v2 states, rejects v1/Scientific and unknown
versions. Empty/default summaries stay v1; lock/join/leave and peer protocols are
unchanged. V1-only summary clients must upgrade for occupied scientific workers.
Workload, plugin, guest ABI and run-descriptor identity bytes are unchanged.

Actual process tests cover opt-in/config precedence, authentication before body,
malformed framing, explicit membership lock, HTTP/2 lookup/body refusal and TLS
HTTP/1+HTTP/2 credential enforcement. A real Newtonian/Euler portable upload reaches
retained initial publication; its receipt survives process restart, while current
run discovery correctly becomes empty. Exact retry ignores replacement artifact
bytes; conflicting roots refuse; interrupted uploads recover Indeterminate. The
three body-adapter tests cover partial frames, EOF signalling, trailers/errors and
fairness. No mock guest or alternate execution owner was introduced.

The current protocol and compatibility decision are in
[scientific-load HTTP v1](../protocol-scientific-load-v1.md) and
[ADR 0031](../adr/0031-expose-bounded-identified-scientific-load-http.md).
Architecture, receipt/run/bundle specifications, worker instructions, task index
and roadmap distinguish this experimental admission subset from public run parity.

Validation: final worker all-target tests passed (218 library tests with two
existing ignored fixtures, 46 binary tests, 43 real client/process tests and
benchmark smoke targets). Shared `orishu` all-target tests passed (101 unit,
seven resource-wire, three run-identity and two load-receipt tests). Shared/worker
all-target Clippy, workspace all-target compilation, worker all-feature/all-target
compilation and shared/worker doctests passed (one existing ignored shared
doctest). Formatting, diff and documentation checks passed (190 Markdown files).
The initial sandbox TLS bind failure was environmental; authorized socket-enabled
reruns passed. The existing `proc-macro-error2` future-compatibility warning remains.
Full workspace runtime tests, separate kernel suites and Kagami GUI smoke were
not rerun for this worker-serving checkpoint; no GUI changes are claimed here.

**Next:** shared-client/Kagami upload/receipt adapters with exact request/descriptor
correlation and bounded responses; then identified public step/stop/unload and
committed observation delivery using the retained owner. No automatic fresh-ID
retry, worker selection change or implicit lock is acceptable. Real bulk HTTP/2
flow-control/disconnection tests remain needed beyond this lookup/auth evidence.
Distributed/reset allocation, durable scientific storage, emitters, runtime RSS/
JIT/reusable-store hardening, cache administration and the earlier full-goal gates
remain open. No registry/discovery scope was added. X-PLUGIN is not complete.

## Shared bounded scientific HTTP client — 2026-09-16

`HttpClusterClient::scientific()` now supplies `submit`, `lookup` and `current`
through the configured worker transport. The caller provides exact immutable
intent and owns retry decisions. Submission consumes one complete portable bundle
and streams its small versioned prefix separately, without another archive-sized
concatenation. The existing CBOR middleware preserves explicit media types while
retaining CBOR defaults for operator calls. Shared transport construction now
disables automatic retries, redirects and automatic decompression, including when
optional compression features are enabled elsewhere in the workspace.

The new path does not use the legacy unbounded generic response helper. It checks
declared and actual streamed byte caps, exact response media, absolute request/body
deadlines, one bounded CBOR value, duplicates, nesting, map/string/work limits and
the narrow receipt/descriptor envelope before accepting data. The complete receipt
request must equal the caller's original intent; accepted descriptor validation
also checks formation/root/nonzero epoch. Pending and refused/indeterminate facts
remain distinct from acceptance. Only 404/OperationNotFound becomes absent history;
an unavailable API, conflict, response mismatch or poisoned-history reply remains
an error. Historical source nodes are not mistaken for current worker identity.

Eight focused client tests cover actual framed bytes and media, all receipt states,
cross-operation/formation/root contamination, misleading statuses, current-run
discovery, malformed/duplicate/oversized/deep/truncated replies, redirects, deadlines
and actual chunked oversized/stalled HTTP bodies. The real worker process journey
now uploads Newtonian/Euler through the shared client, discovers accepted execution,
and retrieves the same historical receipt after restart while current-run discovery
is empty. Raw-wire checks remain independent. A further real TLS test checks the
shared client's explicit certificate trust and accepted/rejected operator tokens;
raw HTTP/1+HTTP/2 credential checks still run alongside it.

Validation: shared all-target tests passed (109 unit, seven resource-wire, three
run-identity and two receipt tests), as did its doctests (one existing ignored).
All 43 worker standalone/process tests passed after shared-client integration;
the real TLS test passed again after adding direct shared-client assertions.
Operator CLI all-target tests passed (six unit and two integration tests).
Shared/worker all-target Clippy, workspace all-target compilation and worker
all-feature/all-target compilation passed. Documentation/format/diff checks passed.
No new dependencies or wire/workload/plugin identity format changes were needed.
The existing `proc-macro-error2` future-compatibility warning remains. Worker
library/kernel suites and Kagami GUI smoke were not rerun for this client-only
checkpoint; their previous results are not claimed as fresh evidence.

**Next:** integrate the shared client into Kagami's headless submission/receipt
commands and window run workflow. Reuse/extract the operator CLI's secure explicit
credential-file loading rather than accept secrets in command-line strings or
invent weaker permissions. Preserve the exact request for uncertain outcomes and
never auto-lock, change worker or generate a fresh retry ID. Public identified
step/stop/unload and committed observations remain necessary for an actual usable
Kagami run. Bulk HTTP/2 flow-control/disconnection proof, distributed/reset scope,
storage, emitters, hardening and earlier full-goal work remain open. This client
adapter is progress toward the original goal, not X-PLUGIN completion.
