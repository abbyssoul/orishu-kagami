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

Current authoring checkpoints below include targeted field parameters, dependency
choices, object-edit history regeneration, explicit component replacement and native
single-template creation. Do not reimplement these from older checkpoint handoffs.
Remaining work includes general compound/migration and catalog-editing/headless
adapters, gesture/configuration-variable effects, other shell pending-effect/IO
reservations and complete run visualization. Durable client recovery is
implemented (see the 2026-10-09 checkpoint). Coordinate
MCP adoption with its separate owner. Source v2 also closes the explicit local
alias/topological-lowering gap; remaining management hardening stays below.
Headless captured-file export and worker delivery/manual control are implemented;
guarded window captured-revision preparation/export/submission, external-run
attachment and numeric controls now use that path.
See [K-RUN](implement-kagami-run-workflow.md) and the checkpoints below. Startup vocabulary and
process-only overrides now reach the actual app authority.
Worker integration now has the formation-owner reservation and off-owner validated
admission handoff from ADR 0028. Actual portable closures reach the shared sandbox,
with parent-linked guest cancellation and reserved owner confirmation. The owner
now allocates canonical run descriptors and non-reusable workload epochs, with
real worker bootstrap/restart evidence (ADR 0029). Lease-bound complete-body IO
with expected-root checks is now implemented. Authenticated request framing,
durable identified command receipts and opt-in daemon serving are implemented. The
retained executor now uses a shared commit gate for owner-serialized initial/step/
stop publication and bounded field leases; see the retained-run checkpoint below.
A confirmed admission or read-only fence check alone is still not commit.
Default startup remains formation-only; explicit scientific enablement now has
real exported-byte, lifecycle, command and observation evidence. This does not
advertise distributed scientific execution.
Installed selections can now feed the shared sandbox through the app's initializer
without hand-assembled reference metadata. Explicit captured-field reinitialization
and whole-setup creation/replacement now adopt through a guarded window effect.
Future configuration/MCP paths must use that gate rather than adopting candidates directly.
The fixed-state reference pair can exercise that real
journey now; do not substitute more isolated fixtures for application adoption.
The remaining full-goal gates below still apply.

1. Source v2 now implements explicit local aliases/topological lowering (see the
   checkpoint below). Complete remaining inventory crash-boundary evidence and
   non-Unix secure IO; origin metadata is implemented.
2. Finish explicit dependency selection and real open-document leases; complete
   UI/MCP management parity through the same local authority. The native panel
   now supplies inspection/install/update/default/enablement and explicit guarded
   live inventory refresh; removal warnings and automatic watching remain.
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

## Kagami headless workload delivery — 2026-09-16

Unix `kagami [-H worker] workload` now supplies `submit`, `receipt` and `current`
commands over the bounded shared scientific client. Submit independently checks
the complete portable closure and explicit expected root before network IO. It
does not open plugin inventory, initialize fields, rewrite a document, lock a
formation, poll automatically or invent retry identity. Operation, formation and
workload identity are caller supplied and retained in uncertain-outcome reports.
Historical acceptance and current executor discovery remain different facts.

The versioned JSON command report has distinct accepted/live, pending, refused,
indeterminate, absent and error outcomes/exit codes. Strings are escaped; reflected
credential material is rejected, including misuse as request identity. Bearer
headers are marked sensitive to prevent ordinary request-debug disclosure. The
existing operator credential loader moved into the already imperative shared
client crate, with the operator CLI delegating to it unchanged. Unix descriptor
checks, bounded reads, exact token spelling and redacted failures are reused, not
reimplemented. No filesystem dependency entered the pure contract crates; no new
third-party versions or workload/plugin/wire identity formats were introduced.

The [command reference](../../apps/kagami/README.md#headless-workload-submission)
documents secure credential/TLS options, explicit request reconciliation and
limits. `make test-kagami-workload` builds the actual worker and obtains its path
from Cargo's artifact output, then explicitly runs the otherwise ignored real
binary journey. The test uploads real Newtonian/Euler Components, obtains the
accepted descriptor, discovers the live owner, restarts the process and verifies
the original accepted receipt survives while no live scientific run is restored.
Ordinary CLI tests cover local refusal before network access, exact framed bytes,
lost replies, all receipt states, mismatched identities and credential reflection.

Validation: four headless CLI tests and four binary parser tests passed; the new
credential-as-intent refusal also passed in a focused rerun. Kagami's 23 library,
15 plugin-management and eight MCP-wire regression tests passed. Shared client
all-target tests passed (112 unit, seven resource-wire, three run-identity and two
receipt tests), as did the operator CLI's three unit/two integration tests and
shared/Kagami doctests (one existing ignored). Scoped all-target Clippy, workspace
all-target compilation, format, documentation and diff checks passed. The actual
worker journey passed initially and again on its final unchanged rerun (104 s).
One intervening run exhausted the test's 80-second receipt wait while still
Pending, during concurrent numerical/JIT work. This is consistent with contention,
not a proven diagnosis; no production deadline or acceptance assertion was relaxed.
Native-JIT interruption/admission-latency hardening remains open. The existing
`proc-macro-error2` future-compatibility warning remains. Full workspace runtime/
kernel suites and GUI smoke were not rerun for this CLI checkpoint.

**Next:** identified public step/stop/unload and committed observation delivery,
then Kagami window submission/run workflow using the same shared client and owner.
This checkpoint delivers admission/reconciliation, not an interactive simulation
workflow. Non-Unix secure IO, distributed/reset allocation, durable scientific
storage, emitters, JIT/RSS/reusable-store hardening, cache administration and the
earlier full-goal gates remain open. X-PLUGIN is not complete.

## Executor-side boundary guards for public control — 2026-09-17

The retained worker executor now provides `step_at` and `stop_at`, preserving an
explicit expected committed boundary until it is checked inside the single
operation slot. The check precedes publication-capacity reservation and all guest
work. A mismatch returns typed expected/actual boundaries without mutation or
terminating the executor. Existing unconditional internal calls remain supported.
An uncertain request must not automatically refresh the expected boundary: that
would transform retry into another scientific step.

The real retained-run tests now exercise future/extreme boundaries, stale step
and stop calls, refusal with a completely reserved formation control lane, exact
stop repetition, busy clones and guarded retry after caller cancellation following
computation. A subsequent explicitly requested step still works. These guards do
not identify which caller committed and are not substitutes for command receipts.

The design/task documentation distinguishes terminal internal integration stop
from the planned resumable public pause/step-budget lifecycle. No public mutation
route, workload format, kernel ABI, numerical formula or scientific time changed.

Validation: all three real-Component retained-run tests passed, including the
extended boundary/refusal/lost-response cases and existing shutdown/owner-loss
publication regression. Worker all-target compilation and Clippy passed; worker
doctests (none defined), format, documentation and diff checks passed. The full
workspace suite and GUI smoke were not rerun for this internal executor change.

**Next:** bounded correlated run-command submission/reconciliation and committed
status projection, then authenticated serving/shared client and Kagami controls.
Preserve exact run identity, explicit boundary and operation intent; do not expose
read-then-unconditional-step or mislabel terminal stop as resumable pause. Public
controls/observations, window integration and the earlier full-goal gates remain
open. X-PLUGIN is not complete.

## Identified run-command facts and journal — 2026-09-17

The shared `orishu::model::run_command` now defines strict versioned request,
committed-status and receipt types. Requests retain operation ID, complete run
identity, exact expected boundary and either one fixed `step` or terminal `finish`.
Epoch zero/impossible next boundaries, invalid SI simulation time and cross-run,
wrong-boundary/wrong-phase application claims are rejected during construction
and serde. Pending, Applied, Refused and Indeterminate remain distinct; status
inside an Applied receipt is historical, not proof of a currently live executor.

The Unix journal IO was generalized behind two sealed concrete profiles, not
duplicated. Load files/schema stay byte-compatible. `CommandReceiptStore` uses
separate names/lock/version and supplies durable intent, exact replay/conflict,
store-incarnation tickets, immutable final outcomes and conservative restart
recovery. It does not execute a command or open itself during worker startup.
Command history has the same explicit 256-record/512-KiB no-eviction limit; sustained
retention and scalable control persistence remain gates, not silently deferred
performance details for a future per-tick loop.

Committed SI time requires finite CBOR floats, so only the command-journal profile
opts into them. Ordinary peer and load encoding/decoding still reject floats;
duplicate/length/depth/work preflight remains active in both profiles. NaN/Inf are
rejected even in the scientific profile. No peer wire, workload identity, kernel
ABI, dependency version or existing journal format was changed.

[ADR 0032](../adr/0032-retain-identified-run-command-outcomes.md) records the
problem/options, and the [v1 fact specification](../run-command-receipts-v1.md)
documents schemas, attribution, recovery and integration obligations.

Validation: three shared command-wire tests, all 15 journal tests (including six
new command/profile tests), all seven codec tests and the peer suite (84 passed,
one existing ignored) passed. Shared all-target tests passed: 112 unit, seven
resource-wire, three run-identity, two load-receipt and three command tests.
The actual daemon-owned admission/lost-response/retained-run regression passed
against real Newtonian/Euler Components after generalizing the journal. Scoped
all-target Clippy, workspace all-target compilation, shared/worker doctests (one
existing ignored), format, documentation and diff checks passed. The existing
`proc-macro-error2` future-compatibility warning remains. Full workspace numerical
and GUI suites were not rerun; no new command execution/serving evidence is claimed.

**Next:** a retained daemon coordinator must own command tickets independently of
response futures, resolve history before live-run eligibility, use exact-identity
guarded execution, classify publication uncertainty safely and publish committed
metadata. Prove response loss, write failure, shutdown/restart and run replacement
before enabling authenticated routes/shared clients/Kagami controls. Continuous
control, public pause/resume/unload, observations, window integration and the prior
full-goal gates remain open. X-PLUGIN is not complete.

## Daemon-owned identified command execution — 2026-09-17

`RunningWorker` can now explicitly install and retain a command coordinator over
its existing load/run authority and an already opened command journal. One
detached, non-queuing operation retains durable intent through exact-identity,
boundary-guarded execution and final receipt IO. Request handle/response loss
does not own the operation. Historical replay/conflict precedes live eligibility;
unload/replacement never rebinds an old request to a newer run.

Applied metadata comes only from an acknowledged identity-checked committed
projection. Executor response/publication loss remains Indeterminate. Final IO
failure poisons history without rolling back a committed step or discarding the
run; reopen converts unfinished intent conservatively. Exact live status is
independent of history health. Daemon shutdown/drop cancels uncommitted work even
with surviving client handles; known committed outcomes still finish receipt IO.
Closing this command lane alone is not an implicit finish/unload.

Validation: all three real-kernel coordinator tests passed (53.47 seconds), covering
dropped responses, replay/conflict, racing boundary changes, terminal finish,
replacement, final-write failure, detached-task interruption, journal reopen and
shutdown before/after execution. The focused uncertainty-classification unit test
also passed. These tests use actual retained Newtonian/Euler runs, not mock
scientific outcomes. Scoped worker all-target Clippy, documentation (199 Markdown
files), formatting and diff checks passed. Startup, HTTP routes and client controls
are unchanged. Full workspace numerical tests and GUI smoke were not rerun.

**Next:** open/install the command journal/coordinator during opt-in scientific
startup, then add authenticated bounded command/status routes and shared-client
correlation before Kagami controls. Sustained history retention, continuous
control, pause/resume/unload, observations, window integration and all earlier
full-goal gates remain open. X-PLUGIN is not complete.

## Authenticated manual commands and shared client — 2026-09-17

Explicit scientific startup now opens both private receipt journals and installs
the command coordinator before listening. Default formation-only startup is
unchanged. The new authenticated POST routes submit exact step/terminal-finish
intent, reconcile command receipts and read committed status for an explicit run
identity. They share the scientific handler budget, strict CBOR/media/length/EOF
rules and metadata deadline. Requests never select a new current run or refresh a
boundary, and request loss cannot dispose daemon-owned command work.

The shared scientific client now implements `command`, `command_lookup` and
`status` with complete receipt/status correlation, bounded responses and no retry,
redirect, implicit locking or polling. Only exact domain absence codes map to
None; missing APIs and uncertain outcomes remain errors. Command/status IO admits
finite CBOR simulation time under structural preflight without relaxing load or
peer float prohibitions. `RunStatusRequest` and new response variants extend the
scientific API; workload/kernel ABI and persisted journal formats are unchanged.

The [HTTP specification](../protocol-scientific-command-v1.md) records exact routes,
schemas, budgets, status/error semantics and lifecycle limits. ADR 0032 now records
the serving refinement and choice of a read-only versioned POST status query.

Validation: all 12 shared scientific-client tests and three command wire tests
passed; the real worker auth/hostile-framing/opt-in test passed. A real portable
Newtonian/Euler workload completed HTTP step, idempotent replay, stale/conflicting
request rejection, terminal finish and process-restart history recovery (27.13
seconds); restarted live status was absent while historical Applied replayed.
Eight worker codec tests and three HTTP-body tests passed. Separate
startup tests also prove corrupt command history fails without data reset/listener
creation and explicit configuration precedence is preserved. Scoped all-target
Clippy, workspace all-target compilation, formatting, diff and documentation
checks passed; the existing proc-macro-error2 future-compatibility warning remains.
Full workspace numerical and GUI suites were not rerun. Command-specific TLS and
HTTP/2 journeys are not separately claimed beyond the reused listener stack.

**Next:** connect Kagami's headless controls to these shared methods with explicit
run/operation/boundary arguments and fact-oriented reporting, then complete its
window/runtime observation workflow. This does not implement continuous execution,
pause/resume/unload, observation delivery, durable scientific restart, distributed
execution or sustained history retention. Prior full-goal gates remain open;
X-PLUGIN is not complete.

## Kagami headless manual controls — 2026-09-17

The existing workload CLI now exposes `step`, `finish`, `command-receipt` and
`status` over the shared scientific client. Controls require exact owner-issued
formation/root/nonzero epoch, operation ID and expected boundary. Receipt lookup
also requires the original action, and never executes it. No command substitutes
current run/boundary, invents an operation ID, retries, polls, changes membership,
opens a plugin inventory or mutates an experiment. Finish remains terminal,
never an alias for pause, unload or continuous run.

The new operations use `kagami.run-command/v1`; existing load/discovery reports
retain `kagami.workload-command/v1` and their shapes. Reports retain original typed
intent after possible delivery, distinguish historical receipts from separately
queried live status, preserve domain outcomes/exit codes and redact reflected
operator credentials. Applied reports use exit 0, Pending 10, Refused 11,
Indeterminate 12 and absent receipt/status 13. Live status includes phase, so a
retained Finished run is not mislabeled as still integrating. Invalid scientific
intent is rejected before networking; missing CLI arguments/zero epoch fail usage.

Validation: four new actual-executable socket fixture tests passed, covering exact
step/finish bytes, lost replies without retry, every receipt outcome, Ready and
Finished status, missing API versus domain absence, mismatched receipts, invalid
intent and credential redaction. The CLI argument test passed. The real
`make test-kagami-workload` journey passed in 27.90 seconds: portable upload,
initial status, one Newtonian/Euler step, replay/receipt lookup without a second
advance, stale/conflicting requests, terminal finish, refusal of a later step,
and restart with historical Applied but unavailable live status. The complete
headless CLI regression suite passed (eight tests; the separately run actual-worker
test is ignored in the ordinary suite), as did all five binary argument tests.
Scoped all-target Kagami Clippy, workspace all-target compilation, formatting,
diff and documentation checks passed. The existing proc-macro-error2
future-compatibility warning remains. Full workspace numerical tests and GUI
smoke were not rerun.

**Next:** implement bounded committed observation delivery and Kagami's run/window
workflow over the shared runtime/proxy boundary; complete local runtime parity
and the remaining authoring/plugin/runtime gates. Continuous control, pause/resume/
unload, sustained history retention and previous full-goal gates remain open.
X-PLUGIN is not complete.

## Committed object leases and independent observer acquisition — 2026-09-17

Before exposing observations, the retained worker's field-acquisition path was
found to consume the same one-operation slot as stepping. That let an observer
cause scientific Busy, contrary to the accepted non-perturbing observer boundary.
Acquisitions now use a separate eight-request nonblocking ingress. Commands are
checked before every observation; readers never own command capacity, block the
executor on delivery, or run sampling guests during acquisition. Idle observation
latency uses the existing 50-ms poll, and command pressure may starve observers.

The shared runtime now exposes complete immutable object/force leases under the
existing field-snapshot count/byte budget. No numerical buffers are copied per
lease. Sources retain exact workload/run-descriptor/epoch/boundary/SI-time;
boundary zero has absent computed forces, and later force packets describe forces
evaluated at the predecessor. Held observations survive advancement and disposal.
Worker `acquire_objects_at` and `acquire_field_at` check the requested boundary
inside the executor; stale requests cannot return a newer snapshot under the
old identity. Queue exhaustion, abandoned reads and lease pressure cannot own
scientific capacity. These are internal host interfaces, not new wire formats.

Validation: all three retained-worker real-kernel tests passed (80.99 seconds),
including observer queue pressure during gated publication, stale acquisition,
shared quota and lease survival. The shared-runtime real-kernel observation test
passed (13.24 seconds), proving immutable object/force buffers, shared field/object
quota, continued advancement and disposal survival. The byte-budget unit test
passed, including exact combined size, failed-acquisition accounting, pointer
sharing, quota return and non-waiting lock contention. Scoped all-target runtime/
worker Clippy passed. The additional late-candidate test passed (40.18 seconds):
an observer queued for a complete candidate receives Closed after shutdown/owner
loss rather than unaccepted scientific bytes. Workspace all-target compilation,
documentation (200 Markdown files), formatting and diff checks passed, with the
existing proc-macro-error2 future-compatibility warning. The real headless
Kagami-to-worker regression was rerun successfully (27.86 seconds). No GUI smoke or full workspace
numerical suite is claimed for this checkpoint.

**Next:** compose these exact committed leases into bounded shared observation
framing/delivery and Kagami adoption. Do not call internal snapshot acquisition
remote streaming, subscription/baseline recovery, recording or window integration.
Those and all prior full-goal gates remain open. X-PLUGIN is not complete.

## Complete-object observation payload — 2026-09-17

The pure shared execution contract now supplies
[object-observation v1](../object-observation-v1.md): canonical committed source
metadata, the existing complete numeric-object packet and optional complete
reduced forces, with SHA-256 identity over the entire bounded frame. Forces are
absent/uncomputed at boundary zero; later frames explicitly expose their N-1
evaluation boundary and require exact dynamic-object coverage. Static objects
remain present. Fields and other authored components are not silently treated as
part of this projection, and binary64 interchange is not a compute-precision claim.

Readers validate length arithmetic, bounds, digest, canonical metadata, every
numeric record and force membership before exposing borrowed data. Exact source
correlation checks workload, descriptor, epoch, boundary and time. Writers reuse
caller-owned storage and leave output unchanged on rejection. The runtime lease's
`encode` method uses this same codec off-executor; no scientific execution formula,
kernel ABI, workload/document format or dependency changed. ADR 0011 records why
raw packets, per-object JSON and whole-run checkpoints were not used as this payload.

Validation: nine codec tests passed, including a pinned 489-byte golden digest,
static/dynamic and empty membership, uncomputed versus empty forces, every source
field, malformed metadata, unknown/duplicate keys, record/byte limits, all truncated
prefixes, trailing bytes, corruption, non-finite/negative-zero values, exact force
membership, output preservation and storage reuse. All plugin all-target tests and
benchmark smoke cases passed, including the dependency guard. The real-kernel
runtime observation regression passed (13.26 seconds), proving that a retained
boundary encodes identically after advancement and owner disposal. Plugin/runtime
all-target Clippy, workspace all-target compilation, docs (201 Markdown files),
formatting and diff checks passed. The existing proc-macro-error2 future-compatibility
warning remains. Full workspace numerical tests and GUI smoke were not rerun.

**Next:** authenticated bounded delivery and client adoption, using this payload
without calling it a complete universe or a stream/resume envelope. Field queries,
subscription identities, baselines, independent delivery budgets and Kagami's
local/proxy window workflow remain open. X-PLUGIN and all prior full-goal gates
remain active; this codec alone is not a user-visible observation feature.

## Authenticated complete-object delivery — 2026-09-17

The enabled worker now exposes `POST /api/v1/run/objects` for an exact
formation/root/epoch/boundary, and the shared scientific client reads/validates
the whole-object payload. The [protocol profile](../protocol-object-observation-v1.md)
records strict request framing, 16-MiB aggregate response policy, source/digest
correlation, deadline/error semantics and the distinction from resumable streams.
No simulation command, initialization, implicit provider choice, boundary refresh
or retry occurs. Finished runs remain readable; an old live run is unavailable
after process restart despite historical command/load receipts remaining valid.

Two observer permits are shared across listeners, independent of the scientific
control handler pool. Encoding runs off-executor. Each encoded allocation owns
its snapshot lease and permit through all shared HTTP chunks, including the final
buffered chunk after producer exit. The send lane has bounded chunks and a deadline;
reader pressure cannot own the scientific command slot. The pure plugin codec is
now a dependency of the shared client, never an execution-engine dependency.
The worker directly names the already-resolved pinned `bytes` dependency for owned
zero-copy chunks. Existing workload, plugin, document and command formats did not
change; only the new versioned read request/route is added.

Validation: the real worker upload/step/finish/restart journey passed (28.36 seconds)
with initial and stepped observations, predecessor-force metadata, stale-boundary
refusal, finished-state reads and post-restart absence. Two authenticated stalled
observers exhausted observer capacity while status and a real step still succeeded;
capacity recovered after they disconnected. The opt-in/auth/framing regression
passed with the new route. A focused encoded-owner test proved final-chunk lifetime
retains capacity after producer/body disposal. All 16 shared scientific-client
tests passed, including four observation fixtures covering exact requests, every
run-scope field, corruption, media/digest/length, absence versus stale/busy/missing
API, actual truncated/stalled wire bodies and deadlines. Request JSON/CBOR version,
unknown-field, duplicate and signed-boundary checks passed. Scoped all-target
Clippy, workspace all-target compilation, formatting, docs (202 Markdown files)
and diff checks passed; the existing proc-macro-error2 warning remains. The real
Kagami CLI-to-worker load/control/restart regression also passed (31.05 seconds).

**Next:** expose these observations through Kagami's client/run projection, then
complete field queries, local/proxy parity and the window workflow. This first
read adapter does not supply subscriptions, delta baselines/resume, recorded
playback or large-response chunk recovery. `ObservedObjects::view` currently
revalidates immutable bytes; repeated consumers should retain one view, not hash
per object. X-PLUGIN and the earlier full-goal gates remain open. GUI smoke and
full-workspace numerical tests were not rerun at this checkpoint.

## Kagami headless committed object inspection — 2026-09-17

`kagami workload objects` now requires an explicit formation/root/nonzero epoch
and committed boundary, then uses the shared authenticated observation client.
Its separate `kagami.object-observation/v1` report retains exact read intent and
validated source/frame identity, complete numeric objects, optional forces and
their predecessor evaluation boundary. Static objects remain present and initial
forces remain null/uncomputed. Success is `observed`/0, absent live run is `empty`/13,
and stale/busy/protocol/transport failures are errors/1 with no possibly-submitted
mutation. Neither history nor a numerical snapshot is reported as live run status.

The command never initializes fields, opens the inventory, submits a scientific
command, edits the document, enters window observation mode, refreshes identity
or retries. It reuses secure credential loading and reflection rejection. Numeric
records serialize through borrowed packet access without extra object/force arrays;
bounded JSON materialization and per-serialization immutable-view validation remain
cold CLI costs, not a renderer performance claim. Existing load/control report
versions and field shapes remain unchanged. Usage, units, scope and limits are in
the [headless inspection guide](../../apps/kagami/README.md#headless-committed-object-observations).

Validation: three actual-executable socket-fixture tests passed, covering initial
and stepped numeric projections, static/dynamic masses, force phase, exact request
bytes, absence/stale/busy/missing API, lost replies, credential reflection and
rejection of corrupt/wrong-boundary payloads without exposing observations. The
real `make test-kagami-workload` journey passed (28.22 seconds): initial objects,
real Newtonian/Euler position changes and forces after step, stale read refusal,
identical retained observations after terminal finish, and unavailable live data
after restart alongside preserved historical receipts. The ordinary headless
suite passed all 11 tests (the real-worker test is separately ignored there),
and all five binary argument tests passed. Scoped all-target Kagami Clippy,
workspace all-target compilation, formatting, docs (202 Markdown files) and diff
checks passed. Existing proc-macro-error2 future-compatibility warning remains;
no GUI smoke or full-workspace numerical suite was rerun.

**Next:** field query delivery and Kagami's actual run/window observation workflow,
with local/proxy parity and the outstanding authoring/management/runtime gates.
One-shot CLI inspection is not a subscription, sensor/MCP integration, baseline
recovery, recorded playback or completion of X-PLUGIN. The full goal remains active.

## Authenticated exact field descriptors and sampling — 2026-09-17

The enabled worker now serves `POST /api/v1/run/field` and `/api/v1/run/samples`;
the shared scientific client validates both. The new pure, bounded canonical
`FieldObservation` descriptor exposes exact committed source/state identity and
selected instance context, never field-private bytes. Sampling reuses OSQ1/OSP1
with observer-generated points, exact typed channels, dimensions, precision,
validity and quality. The [wire profile](../protocol-field-observation-v1.md) and
ADR 0011 record limits, authority and alternatives. A descriptor does not pin the
next request: stale boundaries are explicit refusals, not automatic refreshes.

Both routes share the two-permit observer lane with object reads. Field lease
acquisition uses the existing exact-boundary executor ingress; descriptor encoding,
point scans and disposable-guest sampling run off-executor. Resource permits and
leases outlive a disconnected response waiter and remain owned by every shared
transport chunk. Sampling response delivery shares the runtime output allocation
without another full output copy. Existing native-execution resource caveats remain;
timeouts are not evidence that native work was instantly terminated. Existing guest,
workload, document and command formats are unchanged; only additive versioned
descriptor/read envelopes and routes were introduced. No new dependencies.

Validation: all plugin all-target tests and benchmark smoke passed, including the
new descriptor canonical/truncation/oversize/source/context tests and dependency
guard. All 19 shared scientific-client tests passed: new fixtures cover exact
wire intent, descriptor scope, query/context/state/precision substitution,
corruption/length limits, local refusal before network and absence versus explicit
stale/busy/missing-API errors. Versioned read-intent JSON/CBOR tests passed.
An older oversized-object response fixture was corrected to send a consistent
oversized body instead of inducing a Hyper header/body-length panic.

The real worker load/step/finish/restart journey passed (28.02 seconds), including
initial/stepped sampling, finite values with direct quality, explicit singular and
outside-domain cells, stale-query rejection, forged context/state and malformed/
duplicate-point refusal, unchanged scientific state after reads, retained sampling
after finish and absence after restart. Mixed stalled object/sample uploads share
observer capacity while status and a real step remain usable. That pressure test
initially raced an unobserved early refusal; it now confirms each admitted stalled
body with HTTP 100 Continue before asserting saturation, and passed. Auth/opt-in/
framing and final-chunk budget-ownership tests passed. Kagami's ordinary headless
regression suite passed 11 tests (the separately invoked real-worker CLI journey
was not rerun here). Scoped all-target Clippy, workspace all-target compilation,
formatting, docs (203 Markdown files) and diff checks passed. The existing
proc-macro-error2 future-compatibility warning remains. No GUI smoke or full
workspace numerical suite was rerun.

**Next:** Kagami field-query consumers and actual window run/observation adoption,
with local/proxy parity. Instrument geometry, sensor/MCP integration, explicit
pinned/history/subscription semantics, emitter execution and the previously listed
management/runtime gates remain open. This completes one-shot field transport,
not X-PLUGIN or the full user-requested product workflow.

## Kagami headless field discovery and point sampling — 2026-09-17

`kagami workload field` now reports the exact committed field descriptor and
selected typed observable slots. `workload sample` requires explicit run/boundary,
field instance, observer request ID, ordered channel slots and finite SI points;
it describes that boundary and submits the shared exact-state/context query once.
Neither command opens an inventory/document, initializes fields, chooses another
model, advances science, enters window observation mode or refreshes stale intent.
Usage and output semantics are in the
[headless field guide](../../apps/kagami/README.md#headless-field-inspection-and-point-sampling).

The additive `kagami.field-observation/v1` report preserves original intent and,
on success, exact descriptor, scientific query metadata, request/response digests
and complete requested readings. Scalar/vector/matrix rows retain declared shapes
and dimensions. Valid values carry separate quality flags; invalid cells carry
explicit reasons with no numeric stand-ins. A descriptor acquired before a later
failure may remain visible, without becoming a current-status or success claim.
Credential reflection clears all remote field/sample data and retains safe
original intent. Existing report/wire/workload/document versions are unchanged.

The CLI refuses duplicate/over-budget channels and point counts before connecting,
and absent selected-model slots before issuing the sample request. This initial
CLI is a declared-slot convenience observer, not persistence of probes requesting
unsupported exact scientific contracts. Those lower-level unavailability semantics
remain unchanged. Sample serialization borrows validated packet cells without
per-cell value vectors; bounded JSON materialization and repeated validation remain
cold CLI costs. The actual-executable test harness now drains stdout/stderr while
waiting, with explicit output ceilings, so larger scientific reports cannot
deadlock against pipe capacity.

Validation: all 14 ordinary headless executable tests passed (one separately
invoked real-worker test ignored in that suite). Three new tests cover descriptor
discovery, exact query bytes/point IDs, value/quality/invalidity projection, missing
slots, duplicate channels/4097-point refusal, lost/stale/busy/corrupt replies and
credential reflection. Argument and finite-coordinate parser tests passed.
`make test-kagami-workload` passed (29.13 seconds), exercising initial and computed
field inspection through real Newtonian/Euler execution. Explicit reordered
potential/acceleration/Jacobian queries retained scalar/3-vector/3x3 row-major
shapes, direct quality and singular reasons. Sampling left status unchanged;
stale reads refused, terminal finish retained values and restart returned no live
field, while historical command/load receipts remained readable. Scoped Kagami
all-target Clippy, workspace all-target compilation, formatting, docs (203 files)
and diff checks passed. The existing proc-macro-error2 warning remains. No GUI
smoke or full-workspace numerical suite was rerun.

**Next:** actual Kagami window run/observation workflow and local/proxy parity;
instrument/MCP adoption, subscriptions/history, emitters and the earlier explicit
management/runtime gates remain open. The full X-PLUGIN goal remains active.

## Window remote-run attachment and manual controls — 2026-09-17

The native window now uses the shared scientific client to inspect an external
retained run, explicitly enter Observation mode, refresh committed numeric
objects/forces and issue exact-boundary step/terminal-finish commands. The new
app-local `run` adapter owns one background job and a separate retained projection;
neither network IO nor full-frame validation/table preparation runs per UI frame.
The display labels its 100-object limit, exact provenance and predecessor-force
boundary. It does not render unrelated authoring geometry as simulated output.

Authoring remains gated by the existing session. A late read cannot attach after
panel cancellation, draft edits/replacement or intervening mode changes. Its slot
remains occupied until actual exit. Returning to the open draft discards the run
projection and ephemeral view without sending a finish command or changing the
draft's revision/history/default view. The run is explicitly external: no claim
of document-to-run lineage is made. ADR 0022 records that distinction.

Lost command replies retain the original operation/run/action/boundary. Receipt
lookup and explicit identical resubmission never invent fresh intent; Pending,
absent and Indeterminate remain unresolved. Applied/refused receipts are not
observations, so a fresh snapshot is required before another command. Credentials
are process-local and reflected secrets are refused before projection adoption.
Client recovery state is memory-only and the UI warns to record pending intent
before closing. No wire, workload or document versions changed; no dependencies
were added for this window slice.

Validation: all 17 ordinary workload CLI/window tests passed (one separately
invoked real-worker journey ignored in that suite). New window tests cover mode
gating, unchanged authored state/view, lost replies, absent/Pending/Applied
reconciliation, exact original resubmission, terminal finish, late cancellation/
edit/replacement and wrong-boundary/status-time rejection. All 29 authoring and
five binary argument tests passed. `make test-kagami-workload` passed (29.14s),
including actual window update-path attachment to the computed Newtonian/Euler
run and proof that returning to authoring leaves the worker Ready at its boundary.
Scoped Kagami all-target Clippy, workspace all-target compilation, formatting,
docs (204 files) and diff checks passed. Existing proc-macro-error2 future
compatibility warning remains.

Offscreen renderer smoke passed 120 frames under both projections on llvmpipe
Vulkan. A two-second native-window startup smoke with an isolated temporary
inventory passed with desktop access; its first sandboxed attempt failed to reach
the Wayland compositor. This is startup evidence, not manual visual verification
of the remote-run panel. That visual review and the full-workspace numerical suite
were not performed in this slice.

**Next:** [K-RUN](implement-kagami-run-workflow.md) now specifies the remaining
exact-revision window export/submission, lineage, durable client intent recovery,
3D/instrument projection and local/proxy parity. Streaming/history, emitters and
the earlier management/runtime gates remain open. The full X-PLUGIN goal remains
active; this is the initial window adapter, not completion of the entire product.

## Window captured-revision preparation, export and submission — 2026-09-17

The Unix window now prepares the open captured experiment without saving or
initializing it again. Headless export and the window share `compile_snapshot`:
exact selected kernels, captured scientific inputs and source composition produce
the same deterministic portable bytes. Compilation/packing runs off-window in one
non-queuing lane. Authoring incarnation/revision/mode and final inventory guards
cover adoption. A cancelled/obsolete job retains its slot until it actually exits.
Name/draft changes invalidate readiness; camera changes do not.

The adopted output is explicitly frozen. New-file export refuses replacement.
Submission requires the expected formation and uses the shared authenticated load
API, without implicit locking, initialization, provider resolution or retries.
Original upload intent and bytes survive reply loss and preparation-form changes.
Missing/Pending/Indeterminate outcomes block a fresh load; explicit identical
resubmission preserves its operation, root and bytes. Only terminal history can be
cleared locally. Accepted history permits exact-run inspection, not automatic
observation or recovery of lost worker state. The retained local source incarnation
and revision are associated only with that receipt's exact descriptor; they never
enter scientific identity or imply the current draft is unchanged.

The panel and [K-RUN](implement-kagami-run-workflow.md) document byte budgets and
the memory-only recovery limitation. Preparation admits 128 MiB captured scientific
retention, 64 MiB selected artifacts/8 MiB selected metadata, 128 MiB declared closure
and 128 MiB final archive, with bounded codec/projection intermediates. Upload owns
one additional cold copy of at most 128 MiB while the original is retained for
explicit retry. This is not an aggregate RSS guarantee or an allocation-free upload
claim. No new dependencies, persisted formats, wire profiles or MCP surface changes.

Validation: the seven-test scientific initialization/export suite passed, including
window/headless byte identity, unused-code exclusion in the existing shared export
proof, new-file refusal, unchanged captured source, camera independence and late
draft replacement. The added socket fixture drops an upload reply, returns missing
history then Pending, and verifies identical original bytes/intent on explicit
resubmission after the prepared candidate is discarded. A separate cancellation/
stale-context unit test proves capacity is retained until the blocked job exits.
The first socket run failed because its test expected `/run/loads`; correcting the
fixture to the existing `/run-loads` API resolved it without a protocol change.

`make test-kagami-workload` now exercises two real-worker journeys. The original
CLI/restart journey passed (39.91s in the final run); the window-authored journey
passed (59.98s). That second path creates a plugin-composed dynamic object and
static gravity source, captures fields/history, prepares/uploads the revision,
reconciles acceptance, observes and steps real Newtonian/Euler Components, checks
nonzero attractive force and changed dynamic position, verifies the static source
does not integrate, finishes and restores unchanged authored state/view. Worker
startup receives no plugin inventory. The ordinary workflow suite also passed all
17 tests (one separately invoked worker test ignored), plus 29 authoring and five
argument tests. Scoped Kagami all-target Clippy, workspace all-target compilation,
formatting, documentation and diff checks passed. The existing proc-macro-error2
future-compatibility warning remains. Offscreen renderer smoke passed 120 frames
under both projections on llvmpipe Vulkan, and a two-second native-window startup
with the isolated temporary inventory passed with desktop access. Manual panel visual review and the
full-workspace numerical suite were not performed in this slice.

**Next:** durable client intent/bundle recovery and explicit connection capability
negotiation, 3D/instrument observations and local/proxy parity; dependency-choice/
management parity, object-edit history regeneration, emitter execution and prior
runtime hardening gates remain. The requested full X-PLUGIN goal stays active.

### Committed-object position markers — 2026-09-17

**Progress, not full-goal completion.** The window now renders exact committed
object positions through `kagami-renderer`, alongside the existing numeric table.
Gold denotes Dynamics and blue static/kinematic objects. These are fixed-size,
depth-tested position glyphs, not physical spheres or meshes: the observation
contract does not supply a radius or shape. No authored geometry is used as run
state. The view labels provenance, manual-snapshot freshness and display limits.

At most 65,536 SI positions are extracted off-window per snapshot. Changed scene
scale reprojects the retained SI positions off-window; old-scale markers are
hidden until ready. Capacity and scale-range omissions are counted separately;
geometry is never clamped to a false position. Camera clipping additionally
applies. Immutable batches reuse the GPU instance buffer (maximum 1.5 MiB), with
uploads only on changed batch identity. Camera motion only updates uniforms.
Depth storage follows the window target size. This is single-viewport presentation,
not a claim of zero allocations, aggregate RSS hardening or physical mesh support.

Evidence: the real `SceneProgram`/primitive GPU path passed pixel readback checks
for both projections, depth ordering, behind-camera clipping, unchanged batches,
replacement and removal. `make smoke-kagami` also passed its 120-frame camera
sweep on llvmpipe Vulkan. The window wire fixture verifies scale changes issue no
network calls, preserve SI/provenance and the authored view, report out-of-range
positions and restore them on returning to metre scale. The real-worker window
journey now asserts that Newtonian/Euler integration moves the marker projection
and every marker agrees with its observed SI position at the selected scale.
`make test-kagami-workload` passed both CLI/restart (29.11s) and window-authored
(47.91s) journeys. Ordinary adapter tests passed 17 tests (the separately exercised
worker test is ignored there), plus 29 authoring tests. Renderer and projection
unit tests cover invalid input, bounded capacity and scale conversion. Scoped
all-target Clippy, workspace all-target compilation, formatting, documentation
and diff checks passed. The existing proc-macro-error2 future-compatibility warning
remains. A two-second native-window startup passed; interactive visual review
and the full workspace numerical suite were not performed in this slice.

K-RUN, K-VIEW and the roadmap now distinguish implemented marker presentation
from outstanding field vectors/flow lines, instruments, picking/follow, trails,
streaming and historical playback. Durable client recovery/capability negotiation,
local/proxy parity and the prior full X-PLUGIN delivery gates remain open.

### Native plugin management and explicit availability refresh — 2026-09-17

**Progress, not full-goal completion.** The Unix window now exposes a Plugins
panel over the existing `PluginStore`: all-release listing, verified contribution
inspection, local bundle installation/update, persistent enablement and default
selection. It never executes guests. Opening/refreshing the panel and successful
mutations acquire one coherent inventory revision and adopt component/model
availability without restarting Kagami. Stale commands refuse rather than retry;
process-only overrides remain visible and take precedence over stored preferences.

One off-window job owns capacity until actual completion, even when the panel
closes. The adapter distinguishes an accepted write from a failed follow-up
refresh; it does not tell the user to repeat an accepted mutation. Discovery uses
the existing secure IO and bounded inventory profile, then holds its short final
revision guard through adoption. Changed availability cancels stale scientific
work without releasing its slot early, invalidates prepared workload candidates
and clears unapplied physics-form choices. Exact authored pins, history, saved
dirty state, retained submission bytes and accepted worker runs are not rewritten.
There is no new wire or persisted version and no MCP surface was changed.

The real update-path fixture installs and updates side-by-side bundles, switches
defaults, disables/re-enables exact component availability, inspects declarations,
rejects malformed input and stale concurrent-writer intent, and verifies unchanged
saved experiment/history. A separate fixture verifies persistent enablement cannot
override a process-only disable. A controlled pending-job test proves close keeps
capacity and accepted-write/failed-refresh feedback remains explicit. All 17
management tests, 29 authoring tests and 28 library tests passed. The extended
real-worker window journey disables a local provider during observation, then
successfully advances the same Newtonian/Euler run and restores preferences;
`make test-kagami-workload` passed CLI/restart (29.00s) and window (48.07s) paths.
Scoped all-target Clippy, workspace all-target compilation, docs/diff checks and
offscreen GPU smoke passed; the known proc-macro-error2 future-compatibility warning
remains. A two-second native startup passed. Manual panel interaction/visual QA
and the complete workspace numerical suite were not performed.

Native removal is explicitly unavailable until real open-document/history leases
and acknowledgement warnings are integrated. Remove/pack/validate remain CLI
operations. Open references, full native/MCP parity, dependency-choice dialogs,
automatic watching and the earlier full delivery gates remain open.

### Bounded document-held plugin reference inventory — 2026-09-17

**Progress; live document leases/removal remain incomplete.** A lease primitive
already exists in `PluginStore`, but the application previously had no authoritative
way to identify all releases held by a document session. Scanning visible objects
would miss redo/undo snapshots, captured transitive computational providers and
retained Open/edit request data. Inferring releases from logical names or scanning
unopened files would violate the accepted identity/discovery policy.

`DocumentAuthority::plugin_references` now supplies one pure, cold query over
current objects/scientific selections, both history stacks and retained accepted
requests. It returns deterministic exact-release keys with separate current,
history and receipt presence flags. Work counts empty/legacy structural records
as well as exact contributions; caller-owned work/distinct-release ceilings refuse
an incomplete scan without returning a misleading prefix. Shared history handles
are read without copying opaque buffers, and installed schemas/catalogs never
select a provider for this query. Exhaustive command matching makes new command
variants require an explicit reference-policy choice. No dependency, persistence
or protocol version was added.

The native Plugins panel's **References** action uses this same authority query
and labels it as a point-in-time, this-session-only report—not an acquired lease
or discovery of other files/processes. It runs only on explicit request, not per
viewport frame. A capped query reports unknown/incomplete coverage, never “unused.”

Evidence: four focused session tests cover current/history/receipt separation,
undo/redo, independent receipt eviction, earlier Open requests, exact budget
boundaries, unchanged authority state and no legacy-name fallback. The initial
scientific persistence fixture proved empty rather than exercising a computational
provider; its test now explicitly covers the empty case. The actual Newtonian/Euler
authoring/export fixture separately verifies that the report retains selected
kernel providers absent from every attached object component (passed, 33.93s).
The native management fixture exercises the new action. Document/session unit and
integration suites passed, as did scoped all-target Clippy, workspace compilation,
format/docs/diff checks, offscreen renderer smoke and two-second native startup.
The existing proc-macro-error2 future-compatibility warning remains. Manual panel
visual review and the full workspace numerical suite were not performed.

**Next:** reconcile the complete report with cross-process release leases over
document/undo/replay lifetime, distinguish missing releases from acquisition
failure, and test removal races before exposing native acknowledgement/removal.
An asynchronous lease adapter must guard changes to retained request/history data
as well as document revision: gesture brackets can evict receipts without a new
experiment revision. Do not treat this point-in-time report as that lifetime gate.
Full X-PLUGIN remains active, including the other previously recorded requirements.

### Live document reference leases and native removal — 2026-09-17

**Progress; this lifetime/removal slice is implemented, not full X-PLUGIN closure.**
Inventory-aware Unix windows now retain exact release leases across current
experiment, undo/redo and accepted-request lifetime, regardless of panel visibility.
Before Open or command submission, the document holds a shared `references.lock`
gate. Removal acquires it exclusively under the inventory writer lock; even an
acknowledged removal cannot bypass unknown reference coverage. An authoring action
arriving during removal is refused before document mutation.

One background job captures immutable shared snapshot/request handles, scans with
the existing work/release bounds and adopts exact leases before releasing the
coarse gate. Its generation includes receipt-only changes, not merely experiment
revision. Stale completion cannot unlock newer state; lock/IO/thread/budget failure
preserves protection until explicit retry or another authoring action. Missing
registration is a separate result, never inferred from corruption or acquisition
failure. Existing leases survive acknowledged de-registration and re-registration;
they release after the last retained reference disappears or the owner closes.
Snapshot readers extend old-state lifetime but do not copy scientific buffers or
request bodies. The one-slot bound is not a global memory/RSS proof.

Native **Remove registration…** now stages exact release and inventory revision,
offers unused-only or explicitly acknowledged removal, and explains unchanged
pins, unavailable capabilities and preserved cache bytes. Accepted inventory writes
also invalidate reference coverage when subsequent vocabulary refresh fails.
Shared read-only selection preparation now coexists with reference/revision readers;
writers remain exclusive. No persisted/scientific protocol version or dependency
was added. This new cooperative advisory-lock gate is not honored by old binaries
or direct filesystem modifications.

Evidence: 19 management tests pass, including actual CLI removal during a pending
document scan, acknowledgement, undo/history/request retention and eviction,
re-registration and owner close, native confirmation and unchanged authored data.
Four tracker tests cover stale receipt-only generations, acquisition failure with
explicit retry, reverse removal/edit contention and missing registrations. Five
shared query tests include detached immutable snapshots across later edits. All
document/session unit/integration tests, 32 Kagami library tests, 29 authoring tests
and three scientific-guard tests passed.

The first real-worker window run exposed unnecessary exclusive locking in read-only
selection preparation. A deterministic shared-reader regression failed before the
fix and passed afterward, while asserting inventory writes remain blocked. The
rerun of `make test-kagami-workload` passed actual CLI/restart (31.46s) and window
(71.90s) journeys, the latter including local-provider disablement during an active
worker run. All seven other scientific-initialization tests also passed. An earlier
parallel management run returned a transient `Busy` in the cross-process lease
fixture; isolated, serial and subsequent parallel runs passed. Its cause is not
established, and no production retry or weakened assertion was added to hide it.

Scoped all-target Clippy, workspace all-target compilation, format/docs/diff checks,
offscreen GPU smoke and two-second native startup passed. The known
`proc-macro-error2` future-compatibility warning remains. Manual plugin-panel visual
QA, non-Unix execution and the full workspace numerical suite were not performed.

**Still open:** other adapter/MCP lifetime integration and management parity,
dependency-choice dialogs, origin metadata, automatic watching, broader crash
fault-injection and the earlier full delivery gates. A previously missing release
installed externally is covered only after explicit refresh/reconciliation; these
leases do not discover unopened files or implement physical garbage collection.
Full X-PLUGIN remains active.

### Scientific dependency selection and paged exact providers — 2026-09-17

**Progress; scientific-setup provider choices are implemented, not full X-PLUGIN
closure.** The shared resolver already returned ambiguity and candidate pages;
the window previously flattened setup failure into an unavailable message and had
no way to supply a new explicit dependency binding. Configure physics now exposes
**Check dependencies**, bounded diagnostic/candidate pages and exact provider
selection. Logical plugin owners accompany immutable release/extension/local IDs.
Provider choice is a local proposal; it does not run code or edit the experiment.

The read-only adapter captures model and current scene-component roots, explicit
bindings, document incarnation/revision, form generation and inventory revision.
One job retains its capacity until actual exit. A short shared inventory guard
spans result adoption; stale or failed reads never retry implicitly. Each page has
its own token, preventing delayed index-based clicks from choosing a different
provider. Candidates are replaced per page rather than accumulated. Shared bounds
cover diagnostics, candidates, bindings, dependency work and cold inventory reads;
this does not establish a global RSS budget across other adapters.

Choosing a candidate resets Apply consent and invalidates the report. Explicit
rechecks reveal remaining/transitive requirements; no satisfiable branch is guessed.
Numeric edits preserve chosen pins while invalidating reports; changing models
clears local bindings. Captured settings retain their existing exact bindings.
The same selection builder supplies Check and Apply, including scene roots.
Initialization, document adoption and workload export still independently validate
the complete exact closure. An ambiguous Apply refusal now points to the chooser.
No scientific wire/file version or dependency was introduced.

Evidence: three new production-message-path integration tests pass. A 33-provider
fixture exercises two bounded pages, logical-owner labels, stale page clicks,
invalid offsets/indices, form/document changes, explicit binding preservation and
external inventory revision refusal. Its intentionally non-executable kernel
artifact still permits discovery/resolution, which must not invoke JIT. The real
Newtonian/Euler fixture chooses one of two vocabulary providers across transitive
requirements, initializes, reuses captured pins despite competing defaults,
saves/reopens without inventory, exports the identical selection descriptor, then
admits and advances the workload after the inventory directory is gone (27.30s).

The repeated management-test `Busy` is now diagnosed: Linux `strace` showed another
test thread's `clone3(CLONE_VM|CLONE_VFORK)` inheriting an open inventory-lock
description. The owner closed it, but its next nonblocking lock got `EAGAIN` before
the unrelated child's `execve` completed. This reproduced failures even during
fixture setup. Management fixtures now isolate process spawning from one another;
their intentional thread/process races, exact assertions and production refusal
semantics remain unchanged. All 19 tests passed both normally and under the same
16-thread traced harness. No production retry was added. Local diagnostic traces:
`/tmp/kagami-lock-trace.RUbdSP/{parallel,isolated}.trace` (temporary, not repository
conformance artifacts).

The 32 Kagami library tests, 29 authoring tests and three scientific-guard tests
passed. `make test-kagami-workload` passed actual CLI/restart (28.89s) and window
(47.93s) journeys. Scoped all-target Clippy, workspace all-target compilation,
format/docs/diff checks, offscreen GPU smoke and two-second native startup passed.
The known `proc-macro-error2` future-compatibility warning remains. Manual chooser
visual QA, non-Unix execution and the full workspace numerical suite were not run.

**Still open:** component-only authoring dependency repair and a general explicit
picker for non-default providers absent from the eligible default/already-selected
candidate list; those are not implemented by this scientific-setup dialog.
Scientific/MCP parity, targeted field edits, object-edit history regeneration,
emission/dynamic membership and the other earlier end-to-end requirements remain.
Full X-PLUGIN remains active.

### Explicit installed-provider browsing, including non-default releases — 2026-09-17

**Progress; scientific-setup provider selection now includes non-default releases.**
The preceding ambiguity dialog intentionally used the automatic candidate policy:
enabled defaults and already-explicit providers. That could not select another
installed release unless it was already a root/pin. The shared resolver now exposes
`explicit_provider_page` separately from `candidate_page`. It lists enabled exact
scientific-contract matches, including non-default releases, with the same bounded
ordering, paging, stale-revision and work checks. Neither query selects a provider,
and the automatic resolver's eligibility policy is unchanged.

The window's **Browse installed alternatives** is available for resolved bindings
and reported external requirements. It uses the same one-slot read adapter and
document/form/inventory guards. Installed pages are distinct from automatic
ambiguity pages, replace their bounded candidate list and receive fresh tokens;
delayed clicks cannot select a different provider. Choosing an exact installed
provider changes only a local proposal binding and requires normal revalidation
before initialization/capture. Inventory defaults do not change. Captured pins
remain protected; local-only dependency slots have no alternatives. There is no
new persisted/wire version, dependency, registry or worker inventory.

Evidence: all 11 shared resolution tests pass. The added case proves non-default
browsing leaves automatic resolution unchanged, excludes disabled releases, pages
completely and refuses stale/oversized requests before accepting an explicit pin.
All four window dependency tests pass, including resolved and ambiguous scenarios,
34 installed candidates across pages, stale installed-page clicks, old-release
selection and unchanged default/revision/dirty state. The real Newtonian/Euler
fixture now explicitly selects a *non-default* vocabulary release despite competing
defaults, preserves its complete descriptor through capture/offline file reopen
and workload export, then admits and advances after the inventory is gone. The
newer default is asserted absent from the selected closure (focused rerun 40.44s).

The 32 Kagami library, 29 authoring and 19 management tests passed, as did scoped
all-target Clippy, workspace all-target compilation, format/docs/diff checks,
offscreen GPU smoke and two-second native startup. `make test-kagami-workload`
passed actual CLI/restart (36.93s) and window (65.33s) journeys. The known
`proc-macro-error2` future-compatibility warning remains. Manual browser visual QA,
non-Unix execution and the full workspace numerical suite were not performed.

**Still open:** component-only authoring dependency repair, scientific/MCP parity,
targeted field edits, object-edit history regeneration, emission/dynamic membership
and the earlier full delivery gates. This closes the scientific-setup non-default
provider-picker gap, not the complete X-PLUGIN objective; that remains active.

### Targeted captured-field parameter editing — 2026-09-17

**Progress; the field-only parameter-edit gap is implemented.** The copied-setup
form now retains its source document incarnation/revision and offers separate
per-field reset consent. **Apply only this field's parameters** uses the existing
single, bounded, cancellable scientific-effect slot and final inventory/document
guards. It retains exact provider/instance pins, domain, timestep, precision and
sampling policy. Only the selected field is initialized; unrelated field state
and Dynamics history retain their existing buffers. Replacement parameter metadata
shares the retained-input budget before selection reads/thread execution.

Unrelated pending domain, timestep or other model parameter edits refuse this
narrow operation instead of being dropped or broadening the reset. Whole-setup
Apply keeps its existing explicit all-state reset. Changing any form input clears
field consent; whole-setup consent does not authorize a field-only reset. Copying
again is required after an accepted document edit/reopen or inventory refresh.
Neither failed dimensional validation nor cancellation changes the old capture.
The effect API also requires a current authoring guard and rejects non-field
targets and oversized inputs before preparing kernels. No persisted/wire version
or dependency changed; both paths use the existing undoable setup-adoption command.

Evidence: a new real-Component native-message fixture configures two distinct
field families and a non-empty Dynamics membership. It checks consent, unrelated
pending edit refusal, bad dimensions, stale document/inventory contexts, exact
pointer retention for the untouched field/history, one-revision acceptance,
undo/redo, cancellation, input/target refusals, saved capture reopening and portable
export. A fresh runtime admits/advances that export after the inventory is gone
(final rerun: 45.51s). This covers actual kernels,
not just a mocked callback. Separate metadata-reservation coverage checks bounded
parameter count/text and accounting of owned property/input metadata.

All 33 library tests, 29 authoring tests and four dependency tests passed.
Scoped all-target Clippy, workspace all-target check,
format/docs/diff checks, offscreen GPU smoke and two-second native startup passed.
`make test-kagami-workload` passed actual CLI/restart (29.78s) and worker-backed
window (59.24s) journeys. The known `proc-macro-error2` future-compatibility warning
remains. Native startup is not manual visual/interactive verification of the new
controls. Non-Unix execution and the full workspace numerical suite were not run.

**Still open:** component-only dependency repair, object-edit Dynamics-history
regeneration, emission/dynamic membership, scientific/MCP and management parity,
complete field visualization/instruments, and the earlier end-to-end runtime,
recovery and resource-hardening gates. Full X-PLUGIN remains active.

### Atomic initial-object edits and Dynamics-history regeneration — 2026-09-17

**Progress; configured experiments can now accept object edits requiring new
initial Dynamics history.** The previous authority correctly refused changed
objects paired with an old history-source packet, but the native editor had no
way to supply a coherent replacement. `prepare_history_edit` now reuses the
ordinary command fold, structure/schema/expression/configuration checks and real
allocator high-water marks. Its `HistoryEdit` type exposes only bounded
initialization inputs, never an adoptable experiment or snapshot. The regular
`Candidate` remains available only after complete scientific validation.

The existing single scientific-effect slot now initializes the exact selected
integrator against the proposed initial-entity packet, retains all field captures,
and submits the original edits plus the new capture as one guarded batch. No ID,
component, pose, mass or history is accepted before that complete batch succeeds.
Provider selection and configuration remain pinned. Cancellation, intervening
edits and inventory/context changes discard the whole proposal. Undo/redo restore
captured bytes without invoking kernels. History input and output are separately
bounded; scene proposal admission caps commands/components/properties at 4096
aggregate entries and expression/text content at 1 MiB, with document per-value/
collection limits checked before background work. This is not an aggregate RSS
guarantee.

Existing Unix create/remove, component attach/detach and property actions use the
effect only when normal validation specifically reports changed history. Ordinary
valid edits remain synchronous and need no executable inventory; other validation
errors remain normal authority refusals. The effect API also supports compound
creation and initial transform/velocity edits. The app lane is deliberately
object-only: variable changes affecting captured configuration need their own
affected-capture regeneration plan, not a hidden all-field reset. Dedicated
pose/velocity gestures and scientific MCP routing remain open.

Evidence: the real Newtonian/Euler native-message test covers atomic Dynamics
attachment, mass changes and dimensional refusal, detach/removal, compound
transform/creation with previously deleted allocator IDs, exact field-buffer
retention, undo/redo, cancellation and stale completion after a rename. It tests
the shared preparation directly, including refusal of setup changes/oversized
batches and continued rejection by normal update without replacement history.
The resulting nonempty scene exports and admits/advances after the inventory and
window are dropped (final rerun 68.44s). A compile-fail doctest proves the
preparation cannot yield an experiment snapshot. Input-admission unit coverage
checks oversized text, aggregate counts/text and non-object intent.

Document all-target tests and doctests, session all-target tests, 34 Kagami library
and 29 authoring tests passed. The previous two-field parameter-edit fixture also
passed (52.46s), exercising the factored capture-completion path. Scoped all-target
Clippy, workspace all-target compilation, documentation/format/diff checks,
offscreen GPU smoke and two-second native startup passed. Real worker CLI/restart
(29.96s) and window delivery (48.78s) passed through `make test-kagami-workload`.
The known
`proc-macro-error2` future-compatibility warning remains. Manual interactive UI
verification, non-Unix execution and the full workspace numerical suite were not
performed.

**Still open:** component-only dependency repair, gesture/MCP and
configuration-affecting variable adapters, emission/dynamic membership, complete
field visualization/instruments, broader management parity and the earlier
end-to-end runtime/recovery/resource-hardening gates. Full X-PLUGIN remains active.

### Window field inspection and bounded point sampling — 2026-09-17

**Progress; an attached window can now inspect and sample an exact committed
field without a local plugin installation.** The run panel accepts a configured
field instance ID, obtains its validated descriptor through the shared scientific
client, and lets the observer select declared channels and finite SI point
coordinates. No private field layout is decoded by Kagami. The existing single
background run slot performs descriptor and OSQ1/OSP1 requests; neither read
initializes a kernel, advances the simulation, edits the document nor silently
refreshes to a newer boundary. A descriptor must also agree with the displayed
status's simulation time. Unresolved command intent prevents starting a new read.

Local form/channel generations reject stale successes and errors. Refresh,
scientific commands and detachment clear field observations. Channel actions are
bound to the displayed generation, not merely an index. Query text is limited to
64 KiB, with at most 4096 points and 16 selected channels, additionally subject to
the selected field's shared packet policy. Complete validated packets are retained;
the numeric display projects at most 256 cells and 16 values per cell into flat
storage and cached text off the window thread. Truncation is explicitly labelled
and does not change scientific completeness. Invalid cells remain unavailable,
never fabricated zeroes; quality, declared shape, dimensions, frame and provenance
remain visible. There is no new dependency or persisted/wire format.

Evidence: the actual worker-backed window fixture samples the reference Newtonian
field while its authoring provider is disabled. It checks valid acceleration,
singular/outside-domain invalidity, quality, stale form/channel rejection, invalid
coordinates, display truncation, unchanged document/run boundary and absence of
command intent. An independent second controller advances the worker; sampling
against the window's old descriptor returns `StaleBoundary` without implicit
refresh. Explicit refresh clears observations and selects the new boundary.
The full CLI/restart and window journeys passed through
`make test-kagami-workload` (final window run: 50.01s). An earlier version of the
second-controller fixture failed with `Transport` before its stale-sampling
assertion while reusing a client whose current-thread executor had been parked
during window requests. The final fixture constructs a genuinely independent
client; production transport/retry behavior was not changed, and this does not
claim a pooled-connection defect was diagnosed or fixed.

All 35 Kagami library tests and 29 authoring tests passed. Scoped all-target
Clippy, format/documentation/diff checks, offscreen GPU smoke
and a two-second native startup passed. Native startup is not manual visual or
interactive verification of the new field panel. The known
`proc-macro-error2` future-compatibility warning remains; non-Unix execution and
the full workspace numerical suite were not performed for this increment.

**Still open:** field-instance enumeration for external runs, geometric sampling
generators, persistent instruments, field vectors/flow lines and sensor MCP. This
is a numeric observation prerequisite, not completion of K-OBSERVATION. The
earlier dependency-repair, gesture/MCP/configuration-variable, emitter, management,
runtime/recovery and resource-hardening gates remain. Full X-PLUGIN remains active.

### One-shot field direction glyphs — 2026-09-17

**Progress; validated field samples now have a 3D direction view.** The attached
window offers one exact sampled vector-3 channel, an explicit world-X/Y/Z mapping
action and a finite positive presentation length in metres. Frame/axes/conventions
remain visible; shape alone does not infer a coordinate transform. Arrows are
normalized directions, explicitly not magnitude/quality encoding. The original
numeric values, quality, query/state identities and complete validated packets
remain unchanged. Matrices/scalars cannot be used as spatial vectors.

All queried points participate, independently of numeric-table truncation. Invalid
samples and zero vectors produce no arrows and have separate counters. Checked
SI-to-SceneScale conversion omits out-of-range or collapsed endpoints instead of
clamping them. Normalization scales before squaring so even finite huge and
subnormal values retain direction. One off-window local job reuses the report's
immutable packets without credential IO, a worker request or guest invocation.
Generation guards reject stale results after hide/form/query changes. Scale
changes hide incompatible geometry and reproject once per requested scale;
failed work is not retried every poll. Refresh/commands/detach clear the view.

The renderer consumes generic finite endpoints/RGB, never scientific field
layouts. `ArrowBatch` caps input at 4096 arrows (144 KiB GPU instance storage),
reuses changed-batch uploads and shares the object-marker depth pass. Camera
motion changes uniforms only. Projection is O(points) cold work/storage; GPU draw
is O(arrows). Near/far plane crossings and subpixel/view-axis directions are
deliberately visually omitted. No new dependency, persisted format or wire
operation was introduced. No MCP files were changed.

Evidence: 36 Kagami library, eight renderer and 29 authoring tests passed.
Real worker CLI/restart (29.91s) and window delivery (49.75s) passed through
`make test-kagami-workload`. The window fixture checks actual Newtonian sample
direction and fixed length, singular/outside-domain omissions, stale/unsupported
channel refusal, invalid length, hide during pending projection, all-point output
beyond the truncated table, scale reprojection and unchanged document/report/run.
GPU readback checks both projection modes, arrow/object depth, behind-camera
omission, camera-only reuse, empty replacement and removal. The 120-frame
offscreen smoke, scoped all-target Clippy, workspace all-target compilation,
formatting, docs/diff checks and two-second native startup passed. The known
`proc-macro-error2` future-compatibility warning remains. Native startup is not manual field-panel
interaction/layout verification; non-Unix execution and the full workspace
numerical suite were not performed for this increment.

The first worker-test build stopped in the linker with a bus error before tests
ran, with only 847 MiB free on the workspace filesystem. Six validated old Kagami
incremental-cache directories were removed (about 2.2 GiB, regenerable by Cargo);
source, installed plugins and scientific artifacts were untouched. Linking and
the complete worker journey then succeeded. No production transport change or
automatic retry was used to make the tests pass.

**Still open:** flow lines, magnitude mappings, frame transforms, persistent
instruments and the earlier full X-PLUGIN gates (dependency repair, gestures/MCP,
configuration variables, emitters, management/runtime/recovery/resource hardening).
This is partial K-VIEW slice 4, not completion of K-OBSERVATION or full X-PLUGIN.

### Local plugin acquisition origin and versioned inventory migration — 2026-09-17

**Progress; the accepted local-origin requirement is implemented.** Path-based
package loading now records the bounded absolute input spelling before reading
the explicit source directory/bundle. Origin is separate from canonical roots,
payloads, archive bytes and workload identity. CLI/window install and update share
the same acquisition path. The registration retains its first known origin;
explicit reinstall can fill an unknown value but cannot silently replace a known
one. Byte-only packages and v1 inventories retain unknown origin. No timestamp,
publisher trust, fetch permission or global source-history claim is invented.

Inventory v2 stores optional typed origin with a 4096-byte absolute NUL-free path:
UTF-8 text where possible, lowercase lossless Unix-byte hex otherwise. Inspection
returns this escaped local metadata alongside independently verified cached
package data; it does not follow the path, even after the source is deleted.
Scientific retained-package reads omit origin. Same logical content loaded from
different paths still packs to identical archive bytes and release identity.
The existing 256 KiB index ceiling charges origin too; paths are never truncated
to make a registration fit. The secure writer and nonblocking lock/revision rules
remain in force. Old v1 reads leave bytes unchanged; only an accepted management
mutation publishes v2 at the next revision. Older binaries fail closed on v2.
CLI JSON is explicitly `kagami.plugin-command/v2` for the added local-origin result
fields; consumers requiring v1 must update. Command grammar/error codes and
scientific formats are unchanged. Native Inspect exposes the same record. No
MCP files or worker protocol were changed.

Evidence: all 37 Kagami library tests, 21 plugin-management tests and four dependency
tests passed. Added real CLI/window checks cover acquisition and inspection;
tests cover non-UTF-8 input names, escaped control characters, invalid/oversized
metadata, first-known retention, rejected-install atomicity, source deletion,
origin-free archive identity, retained scientific reads and read-only v1 / accepted
v2 migration. The dependency fixture still captures/saves/exports and executes
without an inventory (28.23s). Actual worker CLI/restart (29.68s) and window delivery
(49.80s) passed. Scoped all-target Clippy, workspace all-target compilation,
format/docs/diff checks, GPU smoke and two-second native startup passed. The known
`proc-macro-error2` future-compatibility warning remains. Manual inspection-layout
verification, non-Unix execution and the full workspace numerical suite were not
performed for this increment.

**Next / still open:** the authoring audit confirms that standalone objects pin
their component contribution but dependency bindings currently persist only in a
captured `ScientificSetup` descriptor. The existing X-PLUGIN authoring slice now
explicitly requires a versioned standalone draft-lock path, coordinated across
save/open, undo, catalogs, references and export, before claiming a durable
component-only chooser. Do not hide this with temporary UI choices or require an
unrelated physics setup. Source-local aliases, exhaustive crash injection,
management/MCP parity, scientific adapters, emitters and the earlier complete
runtime/recovery/resource-hardening and observation gates remain open. Full
X-PLUGIN remains active.

### Standalone provider-intent lock foundation — 2026-09-17

**Progress; component-only authoring integration is not complete.** The shared
`orishu-plugin::authoring_lock` now defines `orishu.plugin-authoring-lock/v1`,
independent of `ScientificSetup` and workload evidence. Private validated storage,
bounded JSON and canonical-CBOR readers/writers retain exact roots, members and
every dependency binding. There is no inventory revision, enablement, code,
domain, captured state or initialization requirement in this format. The
checked-in structural JSON Schema reuses the declaration schema's identifiers;
Rust additionally enforces budgets and graph invariants.

Graph validation rejects duplicate/noncanonical sets, foreign endpoints,
unreachable members and cycles, and checks the complete longest dependency path
with bounded flat scratch storage. JSON refuses an excess array entry before
deserializing its malformed body. `Inventory::resolve_lock` independently checks
current declarations/availability and requires exact closure equality: missing
or disabled providers cannot be replaced by current defaults, and omitted
declared edges cannot be filled implicitly. It rechecks receiving count/work/depth
limits before cloning/traversal. A decoded lock alone grants neither schema
compatibility nor executable admission.

Evidence: seven new codec/graph/schema tests and two new resolver tests passed,
including an independently written empty CBOR golden, independent nonempty CBOR
decoding, malformed/over-budget inputs, shared dependency paths, stale revisions,
changed defaults, disabled/missing providers, omitted/fictitious edges and a real
component-plus-independent-constants choice without fields or kernels. All
`orishu-plugin` all-target tests and its doc test passed, as did scoped all-target
Clippy, workspace all-target compilation, format/docs/diff checks. The known
`proc-macro-error2` future-compatibility warning remains. No new dependencies,
workload identities or existing document/catalog/file versions were introduced.
No worker, renderer or MCP implementation was changed in this increment; their
end-to-end/manual suites and the full workspace numerical suite were not rerun.

**Next:** adopt the lock in the document authority and versioned persistence,
including atomic commands, save/open, undo/redo weight, catalog materialization,
reference leases and exact export reconciliation. Then wire the component-only
chooser to that durable authority. Existing captured `ScientificSetup` bindings
must reconcile with standalone intent rather than become a competing authority.
Do not claim this foundation makes UI-only choices durable or satisfies the full
X-PLUGIN delivery goal; all earlier remaining gates still apply.

### Standalone provider intent in the document and file boundary — 2026-09-17

**Progress; catalog/choice adapters remain open.** `AdoptDependencies` now adopts
a shared immutable lock through the existing atomic document transition. Final
roots must equal the set of exact component contributions attached to objects;
legacy logical references never acquire guessed providers. Component root-set
changes require an updated lock in the same batch, including removals. There is
no implicit pruning or dropping of intent. Captured scientific selection must
contain every locked member and all/only the same dependency edges for those
members; omission of a required edge is not accepted. Scene compilation repeats
the agreement check. Offline hydration requires no installed provider or execution.

The lock is shared across snapshots/undo/redo, and its cached canonical byte
weight joins existing aggregate retention admission across current state,
history and accepted receipts. Receiving authorities recheck their own lock
limits on Open. A rejection preserves revision, counters, events, history and
receipts. Dependency-only providers now enter the existing bounded exact-release
reference projection and therefore window lease reconciliation. Raw lock adoption
is not exposed as an ordinary JSON command; a future UI/MCP choice must still
resolve and guard current availability before submitting the typed command.

[Container v5](../experiment-container-v5.md) persists standalone intent as a
required digest/length-addressed canonical lock blob in the same flat archive as
`document.json` and any scientific captures. Its setup is explicitly legacy or
scientific. It requires no field, kernel or initialization and introduces no
nested archive or new executable identity. Unlocked JSON v3 and scientific v4
writers/bytes are unchanged; old files never acquire a lock on open. Bare legacy
JSON serialization refuses locked documents instead of losing data. Model wire
v4 adds a raw read projection of the lock; session envelope v1 and workload/kernel
contracts are unchanged. Older file readers decline v5 explicitly.

Evidence: all-target tests for `orishu-plugin`, `kagami-document` and
`kagami-session` passed, including existing legacy/v4 golden fixtures. New tests
exercise atomic combined edits, undo/redo, receipts, dependency-only references,
per-lock/aggregate admission, offline round trips, malformed/extra/missing blobs,
root/evidence disagreement and all five durable-write failure windows. Their doc
tests passed. Kagami's 37 library tests passed. The actual installed Newtonian/
Euler dependency fixture saved scientific captures plus an explicit empty component
lock, reopened offline, exported exact selection evidence and ran through fresh
inventory-free runtime admission (28.13s). This proves scientific coexistence, not
a nonempty component-choice UI. Scoped all-target Clippy, workspace all-target
compilation, formatting, documentation and diff checks passed. The known
`proc-macro-error2` future-compatibility warning remains.

The broader scientific-initialization run passed nine cases and exposed one
outdated fixture that treated format 5 as a future version. It now tests the
version after `LOCKED_CONTAINER_VERSION`; this is a fixture compatibility change,
not relaxing unknown-version refusal. The corrected case passed its focused rerun
(43.46s), completing evidence for all ten scientific-initialization cases.
No worker/renderer/MCP implementation was
changed here; worker socket journeys, GPU/native manual checks, non-Unix execution
and the full workspace numerical suite were not rerun for this increment.

**Next:** catalog-scoped dependency locks; pure explicit composition/pruning for
object/template edits; component-only choice UI/MCP using strict inventory
revalidation and document/inventory guards; nonempty composed-object end-to-end
evidence. The earlier emitters, observation, recovery and resource-hardening gates
also remain. Full X-PLUGIN delivery is still active.

### Catalog-scoped provider intent and explicit composition — 2026-09-17

Delivered [catalog v3](../catalog-template-v3.md), extending the document-owned
lock without requiring fields, a solver or a plugin installation:

- `orishu-plugin::authoring_lock` now offers a bounded embedded deserializer,
  borrowed serialization, explicit `for_roots` closure extraction, and `merge`.
  Common exact consumers must agree on all outgoing bindings, including absence.
  Conflicts expose the requirement and both optional providers; no replacement,
  implicit edge repair or current-default selection occurs. Union outputs and
  input graphs are checked against receiving bounds.
- `kagami.catalog/v3` requires structured `spec.dependencies`, including an
  explicit empty lock where appropriate. Roots exactly match template exact
  components. The lock enters canonical bytes/fingerprints, safe writes and the
  materialized candidate. Missing schemas preserve intent as unavailable. V1/v2
  retain their existing bytes and do not silently gain locks. Newly created v2
  provenance now reports v2 rather than incorrectly claiming v1; old historical
  provenance is untouched.
- Lock reads use the default shared ceiling plus tighter receiving policy.
  Direct typed validation cannot raise that ceiling and create a template its
  standard reader cannot reopen. `parse_stream` now checks bytes for in-memory
  callers too. The enclosing YAML loader remains Value-based before lock
  preflight; this is not evidence of fully streaming YAML allocation bounds.
- Document instantiation merges the template lock and object commands in one
  transaction, including aggregate metadata retention, identities, history,
  receipts and events. Unchanged merged intent reuses the current Arc. Conflicts,
  unrelated existing unlocked roots, mismatched captured selection and exhausted
  retention refuse without partial mutation. Explicit removal plus a pruned lock
  uses the same normal command batch. No general UI/MCP auto-pruning is implied.
- Nonempty template instantiation, offline v5 save/open, undo/redo, shared provider
  retention, source-fingerprint staleness and safe catalog writes are exercised.
  The catalog availability projection is schema-only; strict plugin-inventory
  revalidation before computational use remains the consuming adapter's duty.

Validation passed:

- `cargo test --locked -p orishu-plugin -p kagami-catalog -p kagami-document -p kagami-session --all-targets`
  (including 11 shared lock tests, four catalog-lock tests and nine session-lock
  tests; benchmark targets run in test mode, not performance measurements).
- The same four crates' doc tests.
- Targeted Clippy for those four crates plus `kagami`, all targets with `-D warnings`.
- `cargo check --locked --workspace --all-targets`.
- `cargo test --locked -p kagami --lib` (37 tests).
- Formatting, diff whitespace and documentation checks.

One old unknown-version fixture used v3, which is now supported; it now uses v99
and still asserts version-first refusal. The first broad Kagami link failed with
a bus error when the filesystem was full. Six exact generated incremental-cache
directories were moved to `/tmp/x-plugin-build-cache.09Iydu` (about 2.8 GiB, still
recoverable until temporary storage is cleared), with no source/user-data removal;
Kagami's tests then passed. Workspace checking still reports the existing
`proc-macro-error2` future-compatibility warning. Worker/socket/real-Wasm journeys,
GPU/native manual checks, non-Unix execution and the full workspace numerical
suite were not rerun for this increment.

**Next:** component-only resolution/guarded adoption and general root-edit adapters
using these durable locks; real declared nonempty composed-object
capture/export/runtime evidence. Keep all remaining delivery gates above active.
Full X-PLUGIN delivery is not complete.

### Guarded component-only window choices and nonempty execution — 2026-09-17

Implemented the [existing-component Unix choice workflow](../component-provider-choices.md):

- `ComponentForm` copies current exact roots and saved bindings under a document
  guard, independently of fields, domain, integrator selection or initialized
  state. Empty documents and captured scientific setups refuse this form instead
  of creating an unnecessary empty lock or rebinding captured physics. Clearing
  local choices restores copied saved intent.
- The physics dependency controller now serves a small shared proposal interface.
  Component and physics forms share bounded resolution, automatic/explicit
  installed-provider pages and stale-token protection, while retaining separate
  proposal generations. Only the component form may submit `ApplyLock`.
  An explicit discard action removes only a binding the current report identifies
  as unused, so changing an upstream provider can be repaired without silent
  pruning or throwing away all saved choices.
- `PluginStore::prepare_authoring_lock` independently checks the exact closed
  component-root graph, current enablement and declaration bindings, returns
  selected component schemas and retains exact release leases. It uses bounded
  package verification, but builds no execution closure and performs no JIT or
  guest calls. Omitted edges do not acquire default providers during revalidation.
- Apply holds the final inventory guard across the document's incarnation,
  revision and mode check and normal `AdoptDependencies` acceptance. A refused
  completion changes neither document intent nor its schema projection. Accepted
  selected schemas restore unavailable authoring capabilities. The completion
  fold starts reference reconciliation immediately, avoiding a coarse reference
  gate left waiting for an unrelated future user event.
- Saved component bindings enter whole-physics requests automatically. Conflicting
  local physics choices refuse before initialization; they do not override the
  lock. The coordinated migration/reset path needed to change these providers
  after scientific capture remains explicitly unsupported by this form.
- The inspector exposes Load/Check/Browse/Choose/Save without requiring any enabled
  kernels. Saving is dirty/undoable and uses container v5; no persisted format,
  workload identity, kernel ABI or client wire version changed here. No MCP
  implementation was edited.

Real installed-declaration tests exercise an existing component made ambiguous by
installing a second compatible provider, guarded repair, schema restoration,
offline save/open, undo/redo, dependency-only references, stale inventory and
an intervening document edit before the completion fold. Separate preparation
checks cover missing edges, receiving bounds, release leases and disabled pins.

The prior real Newtonian/Euler fixture was strengthened from an empty component
lock to two bodies carrying exact mass and Dynamics components from a deliberately
selected non-default vocabulary release. Their lock is saved through the new
window controller **before** physics initialization. It survives scientific
capture, offline reopening and portable export, excludes unused/default vocabulary
providers, and enters a fresh inventory-free runtime. One actual committed step
gives the two bodies oppositely directed attractive velocities. Existing dedicated
kernel/runtime numerical suites remain the detailed numerical evidence.

Validation includes the nine real plugin-dependency integration cases (including
that Wasm journey), Kagami library/authoring/scientific-guard tests, workspace
all-target checking, targeted all-target Clippy, formatting and docs checks.
Native startup with the isolated existing plugin fixture succeeded and exited
normally after two seconds when given desktop compositor access. The first
sandboxed startup could not connect to Wayland; this was an environment boundary,
not evidence of a working window. Native startup is still not manual inspection
of multi-provider layout, paging or interaction. Existing `proc-macro-error2` and
platform graphics warnings remain. Worker socket/client journeys, the full
workspace numerical suite and non-Unix execution were not rerun in this increment.

**Next:** general root-edit and initially unavailable/unattached-component
adapters; coordinated component-provider migration after scientific capture;
MCP adoption using the same preparation/guards. These remain required, alongside
the broader delivery gates above. Existing-root repair and a nonempty execution
fixture do not close the entire authoring or X-PLUGIN task.

### Atomic native removals in locked documents — 2026-09-17

The native Remove object / Detach component actions now explicitly prepare the
surviving root set and use shared closure restriction before submitting the
ordinary edit batch. Repeated component uses preserve the root and existing lock
allocation; shared reachable dependencies keep their exact providers. Last-use
removal submits the revised lock with the object/component edit. Removing every
root leaves an explicit empty lock, never an implicitly unlocked document.
The authority's strict root-equality rule is unchanged; raw removal without a
matching lock still refuses. This is a local window adapter, not an MCP mutation.

Uncaptured removal requires no inventory or executable, including offline files
whose schemas are unavailable. Captured Dynamics removal now carries bounded
`AdoptDependencies` proposals through the existing guarded history lane. Old and
proposed locks join captured state in its retained-input budget. Normal structural,
captured-selection and history validation still applies; lock, object and matching
history commit together while field buffers remain unchanged. Stale completion
cannot publish either the removal or its pruned lock. Undo/redo restore all three
without executing guests; existing reference reconciliation tracks the new graph
and keeps history references.

Evidence: native-message tests cover repeated roots, shared members also used as
roots, final detach/object removal, invalid repeat, undo/redo, offline removal,
receiving batch-budget refusal and dependency-only current-reference removal.
The real Newtonian/Euler object/history fixture now has a persisted component
lock, exports it for inventory-free execution and checks final-component removal
with unchanged field handles, replacement history and undo/redo. A directly
scheduled pruned-lock/history proposal followed by an intervening authority edit
proves stale refusal independently of worker completion timing. A focused input
test rejects dependency locks exceeding the receiving byte budget before spawning.

Validation: 81 Kagami library/authoring/plugin-dependency/scientific-guard tests
and the real object/history fixture passed. The amended removal and history tests
were rerun after their additional budget/stale assertions. Workspace all-target
checking, targeted all-target Clippy, formatting, diff whitespace and documentation
checks passed. No new persisted format, workload identity or kernel ABI; no MCP
implementation edits. The full workspace suite, worker network journeys, non-Unix
execution and manual window interaction were not rerun in this increment.

**Next:** general root-addition/replacement, first attachment with unavailable or
ambiguous dependencies, coordinated captured-provider migration and MCP adoption.
Removal support does not complete these or the broader X-PLUGIN delivery gates.

### Guarded first attachment and locked-root addition — 2026-09-17

The window now carries the inventory's bounded unresolved component pins through
startup and explicit refresh into the object inspector. **Resolve and add** offers
enabled declarations requiring dependency selection/repair. Adding a new exact
root to an already locked, uncaptured document uses the same staged proposal,
including components whose schemas are already available. Check/Browse/Choose
remain read-only; **Attach component and save choices** submits declared defaults,
the attachment and the complete lock together. The proposal cannot rebind/drop
saved edges, and final graph composition independently requires agreement for
every existing consumer. Existing provider choices are never replaced implicitly.

`DocumentAuthority::submit_with_schemas` provides cold atomic capability-plus-command
admission, reusing the ordinary transition, receipt, event, history and aggregate
retention checks. Failed attachment or retention admission publishes neither
capabilities nor intent/events; replay returns the original receipt without
installing the supplied projection. The guarded app shell uses this same path
for existing-component repair and attachment while the verified inventory guard
and exact release leases remain held. No new wire authority or persisted schema
format was introduced. Undo restores component/lock intent, not installed schemas.

Cancel discards the proposal generation without releasing a running worker slot
early. A stale or cancelled completion cannot install a component, lock or schema.
Required defaults remain required: declarations with missing defaults refuse
without inventing values or leaking their prepared capability projection.

Evidence includes native-message initial ambiguity/selection, new roots from
another release, a selected dependency promoted to a root, preservation of saved
edges, undo/redo, deterministic stale/cancelled completion and required-default
refusal. Core authority tests cover complete view/history/event/schema rollback
on retention refusal, acceptance, replay with different supplied schemas and
undo/redo. Validation passed: 184 session all-target tests, its doctest, 84 targeted
Kagami library/authoring/plugin-dependency/scientific-guard tests (including the
real selected-closure export/runtime fixture), workspace all-target checking,
Kagami/session all-target Clippy, formatting, diff whitespace and docs checks.
The full workspace suite, worker network journeys, non-Unix execution and manual
window interaction were not rerun. No MCP implementation was edited.

**Next:** uniform lock capture for already-available components added to unlocked
documents (the earlier synchronous path is unchanged); property entry when
required defaults are missing; general component replacement; coordinated
captured-state additions/provider migration; MCP adoption. The standalone
attachment path now works, but these and the wider X-PLUGIN delivery gates remain.

### Pin choices on the first available native attachment — 2026-09-17

Native exact-component Add now stages dependency resolution whenever an
uncaptured document has no standalone lock, even if the component schema is
already available. An available schema is no longer mistaken for durable
provider intent. Acceptance uses the existing leased/guarded schema-component-lock
transaction and saves the complete exact graph for all current exact roots plus
the new component. Another use of an already locked root can retain the existing
synchronous path because it introduces no new choice. Legacy logical actions and
offline reading of older unlocked documents/templates are unchanged; no provider
is guessed for a logical ID and no read silently migrates a file. Captured-state
additions still require their separate coherence/migration work.

The inspector explains Check/Save before attachment. Real startup/management and
Newtonian/Euler fixtures now exercise this two-phase native path, rather than
assuming first attachment is immediately accepted. Legacy repair fixtures are
explicitly seeded through the typed authority as older unlocked documents, so
they continue testing the intended compatibility case instead of bypassing the
new UI behavior unnoticed. A focused fixture accepts a first available component
with one dependency, then installs a second compatible provider: recheck retains
the original binding without ambiguity, and offline save/open preserves it.

Validation passed: 106 Kagami library/authoring/scientific-guard/plugin-management/
plugin-dependency tests, including actual selected-closure export and fresh
runtime execution; workspace all-target checking; Kagami all-target Clippy;
formatting, diff whitespace and docs checks. `make smoke-kagami` passed on
llvmpipe, including both projections and marker/arrow pixel checks. This is not
manual inspector interaction/layout QA. Full workspace tests, worker network
journeys and non-Unix execution were not rerun. No MCP implementation was edited.

The build volume had less than 1 GiB free. Three exact generated Kagami incremental
cache directories were moved recoverably to `/tmp/x-plugin-incremental.vaAyOo`;
source files and build outputs were not removed. Existing future-compatibility
warnings remain unrelated to this change.

**Next:** property entry for components lacking required defaults; general
replacement; coordinated captured-state additions/provider migration; MCP
adoption and the wider X-PLUGIN runtime/delivery gates. Native unlocked first
attachment no longer remains a provider-intent gap.

### Bounded attachment property entry and read-only field inspection — 2026-09-17

Every uncaptured exact attachment now offers a local property proposal, including
another use of an already locked component. This closes the required-no-default
case for second and subsequent objects too. Available declarations populate
bounded fields/defaults immediately; an unavailable declaration can be inspected
with **Load property fields** after dependency resolution. That guarded read
revalidates the selected closure and copies only local declaration metadata; it
does not install a schema or modify the document. Apply revalidates availability
and exact schema agreement again before the existing atomic schema/component/lock
acceptance. Identical accepted locks reuse the existing retained allocation.

The inspector supports retained quantity expressions, literal text and explicit
boolean values, including empty text/false versus Unset. Requiredness, dimensions,
expression evaluation and plugin constraints are normal authority checks. A
failed save retains local values for correction without leaking a component,
schema, history entry or receipt. Offline missing schemas preserve quantity
sources as unevaluated values, not fabricated numeric evaluations. Undo/redo
restores accepted object/lock contents; installed capabilities remain separate.

Property messages carry a distinct form identity; local value changes advance
proposal generation and invalidate pending Apply/resolution. Cancelled/stale
field reads cannot populate a new proposal or install capabilities. Field/source
retention is bounded: at most 256 declaration fields, receiving property-count,
expression/text limits, and a 1 MiB aggregate source ceiling. Defaults are checked
before cloning; rejected edits preserve the previous local values. This is not a
whole-process memory bound or a claim of complete property editing in other adapters.

Evidence: a real installed declaration with missing quantity/text/boolean defaults
refuses incomplete input, allows read-only field inspection, rejects wrong units
and text constraints, then accepts `1 kg + 1 kg`, explicit false and empty text in
one revision. Tests cover offline sources, undo/redo, a second object with distinct
values and unchanged provider choices, edits during in-flight Apply, stale/cancelled
property reads, cross-form messages, wrong-kind/oversized inputs, aggregate source
limits, field counts and absent/default distinctions. The existing real selected
workload export/runtime fixture still passes with the staged attachment path.

Validation passed: 110 focused Kagami library/authoring/plugin-dependency/plugin-
management/scientific-guard tests, workspace all-target checking, Kagami all-target
Clippy, formatting, diff whitespace and docs checks. Renderer smoke passed on
llvmpipe under both projections. Full workspace tests, worker network journeys,
non-Unix execution and manual inspector interaction/layout QA were not rerun.
No MCP implementation, persisted format, workload identity or kernel ABI changed.

**Next:** general component replacement; coordinated captured-state additions and
provider migration; MCP adoption and the wider X-PLUGIN execution/delivery gates.
This completes the uncaptured attachment property-entry path, not the whole goal.

### Explicit uncaptured component replacement — 2026-09-17

The Unix object inspector now offers **Choose replacement…** on an existing
component. Selecting a different exact component creates a guarded local proposal;
it does not detach the original. New values come from the new declaration's
defaults or explicit input, never inferred copying/conversion of old properties.
The shared dependency/property workflow independently checks the selected closure
and declaration. Acceptance submits detach, attach and revised lock as one atomic
schema/command operation, with existing availability guards and release leases.
Successful acceptance clears the chooser; refusal retains values for correction.

Preparation restricts old provider intent to surviving roots, also retaining any
selected dependency promoted to the new root. Shared consumers retain complete
bindings. Only intent made unreachable by the explicit replacement is removed.
Another object's use of the old component preserves its root and providers.
Replacement does not require a spare component slot: the bounded command batch
detaches before attaching, without exposing intermediate state. Undo/redo restore
objects, values and locks together; offline save/open preserves accepted choices.
Captured scientific setups still refuse this form and require coordinated migration.

Evidence: real installed declarations exercise new and promoted roots, shared and
last-use replacement, explicit quantity values, offline reopen and undo/redo.
Invalid values, cancelled/stale completions and expired picker identities cannot
detach the original. Tests also cover ending replacement mode after acceptance,
local preparation at the component-count limit and refusal after real scientific
capture. Existing selected-workload export and inventory-free execution still pass.

Validation passed: 113 focused Kagami library/authoring/scientific-guard/plugin-
dependency/plugin-management tests, workspace all-target checking, Kagami all-target
Clippy, formatting and documentation checks. Renderer smoke passed 120 frames and
marker/arrow checks under both projections on llvmpipe. Full workspace tests,
worker network journeys, non-Unix execution and manual inspector layout/interaction
QA were not rerun. No MCP implementation, persisted format or kernel ABI changed.

An initial native test link failed with a linker SIGBUS while the build volume had
less than 1 GiB free. Three exact generated incremental caches were moved
recoverably to `/tmp/x-plugin-link-cache.HIOCpc`; the retry passed. Disk pressure
was suspected, not proven as the cause. No source files were removed.

**Next:** coordinated captured-state additions/component-provider migration; MCP
adoption and the wider X-PLUGIN execution/delivery gates. General uncaptured
replacement is no longer an open adapter gap; the overall goal remains active.

### Captured-component extension preparation and atomic adoption — 2026-09-17

`kagami_document::update::prepare_extended_history_edit` now prepares component
additions against an extended verified declaration set without exposing an
incoherent experiment. Existing roots, members, complete provider bindings and
kernel-instance identities must remain unchanged. The core verifies retained
captures against the new declarations and uses the same ordinary command fold,
schema governance and expression/configuration checks as history editing. Only
numerical-history coherence is deferred; normal guarded acceptance still requires
the complete new capture. This does not authorize provider or kernel migration.

The internal Unix `ScientificEffects::extend_components` lane now takes exact
attachment commands, a complete component lock and the originating authoring guard.
It bounds input before starting work, independently revalidates the lock/schemas,
merges saved intent without rebinding, retains exact release leases and compiles
the complete selected closure with the existing kernel uses. It regenerates
integrator history from the proposed objects using the same initializer as ordinary
scene edits. Field buffers, domain, timestep and compute settings are retained.
Final inventory/document guards cover atomic adoption of attachments, lock,
extended scientific evidence/history and selected schema capabilities. Cancelled,
stale or refused completion cannot install a schema or accept a partial edit.

Evidence: a real captured Newtonian/Euler setup with two initially static objects
receives Dynamics, gravitational mass and a newly installed additive component
whose dependency references Dynamics. The new schema is absent before acceptance;
afterward the revision, components, lock and scientific evidence change together,
while field bytes retain their allocation. History contains the two dynamic IDs.
Undo/redo and offline save/open preserve the whole transition. Exported bytes admit
and execute in a fresh runtime after the document/inventory are dropped, producing
oppositely directed attractive velocities. Invalid units, cancellation, an
intervening edit, command-budget overflow and non-attachment requests refuse.
Core tests independently reject changed kernel uses and changed provider bindings
even with valid compiled declarations and every old member still present.

Validation passed: 126 Kagami library/authoring/scientific-guard/plugin-dependency/
plugin-management/scientific-initialization tests; 318 document/session all-target
tests and benchmark test-mode checks; three document/session doctests; workspace
all-target checking; affected-crate all-target Clippy; formatting, diff whitespace
and docs checks. The real scientific regression suite ran with two test threads.
Full workspace tests, worker network journeys, non-Unix execution and manual window
QA were not rerun. Rendering/UI code was unchanged in this increment. The existing
`proc-macro-error2` future-compatibility warning remains. No MCP implementation,
persisted format, workload identity rule or kernel ABI changed.

**Next:** connect the native captured Add proposal to this lane. Retain bounded
property/dependency editing and complete provider choices, require explicit consent
to regenerate history, and carry document/form/inventory guards through the
asynchronous handoff. The current inspector still refuses newly introduced captured
roots; the internal API is not completed user-facing parity. Captured-provider
replacement/migration, MCP adoption and wider X-PLUGIN execution/delivery gates
remain open. The overall goal remains active.

### Native captured Add with proposal-specific consent — 2026-09-17

Every native exact Add now stages component properties and complete dependency
choices, including attachment of a root already used on another object. For a
captured experiment, the proposal preserves reachable captured component choices
and refuses rebinding existing scientific consumers. New property values still
come from exact declarations/defaults or explicit input. Loading an unavailable
declaration remains a read-only local operation, not capability adoption.

Captured Add requires explicit consent to regenerate initial integrator history
while preserving fields and existing providers. Consent names the exact local
proposal generation; editing values or provider choices invalidates it, and old
checkbox messages cannot authorize later inputs. Checked component intent hands
off to `ScientificEffects::extend_components` in the same update fold, with
document/form/inventory/consent checks. The scientific lane independently repeats
availability/selection validation and accepts components, lock, declaration
evidence, history and schemas atomically. No direct component-only adoption occurs
for captured additions.

The window retains ownership of the scientific operation through its actual exit.
Cancel or withdrawn consent prevents acceptance before or after the dependency-to-
scientific handoff; pending work continues to own capacity until cleanup. Successful
acceptance ends the proposal. Failure leaves local values available for correction
but requires fresh consent and an explicit retry. The scientific polling
subscription remains active after dependency discovery releases its slot.

Evidence: native-message tests load an unavailable component's fields without
installing its schema, invalidate consent on input edits, refuse old checkbox
messages and unconsented Apply, withdraw consent after handoff, cancel a pending
dependency read and preserve intervening edits on both sides of handoff. Successful
attachment advances one revision, retains field buffers, saves/reopens offline,
exports, and restores the full change through undo/redo. Another object receives
the same locked component with distinct values and the same lock allocation.
The existing real Dynamics object/history journey now uses the consented native
attachment path; shared real-kernel export/admission/execution regressions pass.

Validation passed: 128 Kagami library/authoring/scientific-guard/plugin-dependency/
plugin-management/scientific-initialization tests, workspace all-target checking,
Kagami all-target Clippy, formatting, diff whitespace and docs checks. Renderer
smoke passed 120 frames plus marker/arrow checks under both projections on llvmpipe.
Full workspace tests, worker network journeys, non-Unix execution and manual
inspector interaction/layout QA were not rerun. No MCP implementation, persisted
format, workload identity rule or kernel ABI changed.

The first native test link failed with linker SIGBUS while the build volume was
low on space. Three exact older generated worker incremental caches were moved
recoverably to `/tmp/x-plugin-native-cache.AZ5NYa`; the retry passed. Disk pressure
was suspected, not proven as the cause. No source files were removed.

**Next:** captured provider replacement/migration, compound/catalog and headless
composition adapters, MCP adoption and the wider X-PLUGIN execution/delivery gates.
Ordinary native captured component attachment is no longer an open adapter gap.
The overall goal remains active.

### Explicit captured-provider full-reset backend — 2026-09-17

Implemented `ScientificEffects::reset_components` and `ComponentResetRequest`.
The caller supplies explicit scene edits/new values, the complete resulting
component lock and a complete new physics request. This is a distinct operation
requiring consent to discard all old field/history state, not implicit migration
or an extension of ordinary Add consent. Its native proposal/consent and MCP
adapters remain unimplemented.

Shared `prepare_scientific_reset` folds ordinary commands using the real source
allocation counters. Schemas, expressions, bounds, exact component roots and
complete lock agreement with new verified scientific evidence remain mandatory.
Only old capture coherence is deferred inside this non-adoptable preparation;
it exposes variables and projected Dynamics input, not a snapshot or old state.
Setup/domain/timestep commands cannot bypass the separately supplied reset request.
Final acceptance resubmits edits, lock and the complete new setup through normal
authority validation with staged schemas.

The backend independently revalidates authoring and scientific selections,
retains their release leases, initializes every field/history afresh and keeps
the inventory revision guard through acceptance. It shares full initialization
with ordinary setup creation/replacement. Fields receive no coupled objects in
initialization; history receives newly projected objects under the new bindings.
Neither old executables nor old blobs are needed to initialize new providers.
Aggregate owned configuration weight across all kernels now reduces the admission
budget for retained old scientific inputs; scene inputs retain their separate
bounded admission. This is not a whole-process RSS guarantee.

Real Newtonian/Euler tests replace both vocabulary and solver providers, disable
both old packages, explicitly change component mass/configuration/timestep, and
accept new components, choices, schemas and fresh captures in one revision.
They prove undo/redo, offline v5 reopen, portable export and fresh inventory-free
runtime admission/attraction. Core tests reject ordinary acceptance with the old
capture, expose bounded new Dynamics input only, and refuse setup commands during
preparation. Invalid values, old locks, missing edits, scientific-selection
disagreement, later field-kernel refusal after history initialization, cancellation
and stale edits preserve the source and install no new schemas. Command-count
and embedded-lock refusals occur before starting a worker.

Validation passed: 130 Kagami library/authoring/scientific-guard/plugin-dependency/
plugin-management/scientific-initialization tests; 318 document/session all-target
tests plus benchmark test-mode checks; four document/session doctests; workspace
all-target checking; Kagami/document/session all-target Clippy; formatting, diff
whitespace and docs checks (207 Markdown files). The 17-test real scientific suite
ran with two test threads and completed in 327.98 seconds. The initial focused run
had one assertion expecting a different diagnostic phrase; the kernel correctly
refused the input and left state intact. The corrected assertion and all subsequent
runs passed. Full workspace tests, worker network journeys, non-Unix execution and
manual window QA were not rerun. Rendering/UI code was unchanged in this increment;
no renderer smoke was needed. The existing `proc-macro-error2` future-compatibility
warning remains. No MCP implementation was changed.

**Next:** connect a native captured replacement proposal with complete new
component values/provider/physics choices, explicit loss-of-state preview and
input-specific full-reset consent. Keep cancellation and document/form/inventory
guards across both preparation stages; leave ordinary Add field-preserving.
Compound/catalog and headless composition, MCP adoption and the wider X-PLUGIN
execution/delivery gates remain open. No persisted format, workload identity rule
or kernel ABI changed. The overall goal remains active.

### Native captured replacement and binding-only full reset — 2026-09-17

Connected native authoring to the full-reset backend. An author may choose a new
exact component for one selected object, with explicit values, or load captured
component choices to change dependency providers without object edits. Captured
choices seed the proposal; explicit reset rebinding is allowed, while ordinary
captured Add remains field/provider-preserving. No property conversion is inferred.

The component resolver stages checked intent without adopting commands or schemas.
The inspector then presents a complete physics proposal: domain/grid/timestep are
copied, but models, parameters, precision and sample policy are not carried over
implicitly. The author selects all desired fields/integrator and checks physics
dependencies against the proposed component lock, not the superseded document
roots/bindings. Final Apply freezes the exact displayed resolved graph and invokes
independent backend revalidation and fresh initialization.

Full-reset consent and Apply identify both component and physics generations.
Physics changes revoke consent; component changes discard the staged physics too.
Old buttons/checkboxes cannot authorize later inputs. Ordinary physics/field/Add
consent is not reset consent. Cancellation and withdrawal work before and after
handoff, with worker ownership retained until actual exit; document/mode/inventory
guards invalidate stale proposals. Failure requires explicit confirmation/retry,
and success ends the proposal after one atomic revision. No new schemas or partial
component/provider changes are accepted during preparation.

Native-message tests use installed declarations and real Newtonian/Euler kernels.
They exercise read-only field loading, staging without adoption, mandatory explicit
model choices, wrong/stale consent and Apply, physics/component edits, cancellation
during dependency work and after scientific handoff, late kernel refusal, and
intervening edits. Successful replacement preserves the other object's old
component, adopts explicit new values and new kernel providers, resets every
capture, supports undo/redo/offline reopen/export, and reaches fresh inventory-free
runtime admission/attractive motion. A binding-only journey uses installed-provider
pages to change a captured dependency, explicitly resolves the integrator's
different provider choice, and resets without changing any object data.

Validation passed: 133 Kagami library/authoring/scientific-guard/plugin-dependency/
plugin-management/scientific-initialization tests; a focused rerun of all three new
native reset tests after the final preflight-consent guard; workspace all-target
checking; Kagami all-target Clippy; formatting, diff whitespace and docs checks
(207 Markdown files). The full 20-test scientific suite completed in 436.38 seconds
with two test threads; the focused final rerun completed in 161.59 seconds while
both suites overlapped. Renderer smoke passed 120 frames plus marker/arrow pixel
checks under both projections on llvmpipe. Full workspace tests, worker network
journeys, non-Unix execution and manual inspector interaction/layout QA were not
rerun. The existing `proc-macro-error2` future-compatibility warning remains.

**Next:** general compound/catalog and headless composition adapters, MCP adoption,
gesture/configuration-variable authoring and the remaining X-PLUGIN delivery gates.
The native selected-component replacement and binding-only reset gaps are closed;
this is not a general multi-object migration planner or selective state migration.
No persisted format, workload identity or kernel ABI changed. No MCP implementation
was changed. The overall goal remains active.

### Captured compound/catalog addition preparation — 2026-09-17

Extracted the existing catalog-to-command translation into pure
`kagami_session::instantiation::prepare`, now used by ordinary session acceptance
and captured-scientific addition preparation. It takes an explicit immutable
catalog and the original experiment, uses real allocation counters for object-local
definitions, preserves rewritten sources/provenance and merges complete template
locks without replacing intent. Caller binding count/source bounds precede copying;
materialized command counts are checked before constructing the command vector.
Preparation never mints or adopts IDs. Normal authority validation remains the
only acceptance path; ordinary instantiation does not bypass captured history.

`ScientificEffects::compose_scene` now accepts bounded exact component attachments,
new objects and new definitions with a complete resulting component lock.
`instantiate_template` retains one adopted catalog snapshot and requires an exact
fingerprint, available entry and independently verified complete lock. Catalog
reload after submission does not replace the retained source. Template bindings
cannot be overridden by a different supplied lock, even if that lock resolves
successfully; dependency-composition refusals retain the typed conflict. Existing
providers/kernels/fields/settings survive, while integrator history is regenerated
and copied data, choices, evidence/history and schemas accept atomically under the
existing inventory/document guards. The attachment-only API stays attachment-only.
Source/entry and aggregate effect bounds precede retention/preparation as applicable;
catalog materialization retains its existing loading limits, not a new RSS guarantee.

Real Newtonian/Euler tests cover repeated catalog instantiation after deleted IDs,
parameter overrides, clearing the catalog while pending, object-local definitions,
unchanged field-buffer handles, undo/redo, offline v5 reopen, portable export and
fresh inventory-free runtime admission/attraction. Compound creation accepts two
objects and a new definition in one revision; a duplicate definition refuses
without mutation. Other tests cover stale fingerprints, invalid bindings,
cancellation, intervening document edits, absent fingerprints, command/source/
aggregate bounds, and a valid alternate provider lock conflicting with the selected
template. No failed proposal installs new schema capabilities.

Validation passed: 137 Kagami library/authoring/plugin-dependency/plugin-management/
scientific-guard/scientific-initialization tests, including the 23-test real suite
(502.91 seconds, two test threads); all four focused catalog/compound tests
(107.61 seconds while overlapping the broader suite), followed by the final
provider-conflict diagnostic test (24.96 seconds). The latter is an additional test
beyond those 137. Session all-target tests and its doctest passed, including 12
instantiation/preparation tests. Final Kagami/session all-target Clippy, workspace
all-target checking, formatting, diff whitespace and docs checks (207 Markdown
files) passed. Initial test compilation and one test-only Clippy issue were fixed.
A final native link crashed with a nearly full workspace volume; moving six stale
incremental cache directories to `/tmp/x-plugin-catalog-cache.r5Pxda` freed about
1.5 GiB and the retry passed. Those caches remain recoverable; no source was removed.
The existing `proc-macro-error2` future-compatibility warning remains. Full workspace
tests, worker network journeys, non-Unix execution and manual window QA were not
rerun. This increment changes no renderer/widgets, so no new renderer smoke ran.

**Next:** native catalog loading/selection, bounded parameter/placement input,
complete provider choices and proposal-specific history consent, connected to this
backend without an implicit provider reset. Unavailable catalog entries require
explicit revalidation under selected schemas, not silent promotion. Preserve the
document/catalog/form/inventory context across reads and scientific handoff.
Headless/MCP adoption, general migration planning, gesture/configuration-variable
authoring and the remaining X-PLUGIN execution/delivery gates stay open. These
preparation APIs are not a native catalog browser, emitter implementation or MCP
parity. No persisted format, workload identity or kernel ABI changed. No MCP code
was modified. The overall goal remains active.

### Native catalog creation and guarded availability refresh — 2026-09-17

Connected the Unix native Catalog panel to the existing catalog authority and
shared instantiation preparation. Users can load/reload a directory, page bounded
entries, select an exact template fingerprint, edit instance name/placement/velocity
and parameter expressions, resolve complete provider choices and create an object.
Structurally valid unavailable templates remain inspectable; independently verified
selected schemas revalidate the retained source before acceptance. The picker never
guesses a provider, replaces existing/template bindings or writes catalog files.
Uncaptured creation atomically adopts copied data, verified schemas and dependency
intent without executing kernels. Captured creation preserves field state/providers
and enters the guarded history-regeneration lane with proposal-specific consent.

The authority supplies shared immutable snapshots. Reload with current schemas uses
the same expected-revision/replay checks as normal catalog commands; changing the
read projection does not dirty the experiment. Directory enumeration now has a
separate bound, including ignored entries, and refuses an over-budget scan instead
of presenting a filesystem-order-dependent prefix. Instance inputs and asynchronous
jobs retain explicit bounds. Stable input identities allow queued edits, while a
separate proposal generation invalidates stale provider reports, Apply and consent.
Cancellation, consent withdrawal and changed document/inventory/input context cannot
publish data or schema capabilities; cancelled work retains capacity until exit.

Real native-message tests cover unavailable-schema recovery, locked-provider refusal,
unlocked ambiguity and explicit choice, input bounds, stale selection/Apply/consent,
invalid expressions, cancellation across scientific handoff, undo/redo and offline
reopen. Captured native creation reaches portable export and fresh inventory-free
Newtonian/Euler runtime execution. Reload refreshes availability under current
schemas without changing previously created objects or retained source snapshots.

Validation passed: 142 Kagami library/authoring/plugin-dependency/plugin-management/
scientific-guard/scientific-initialization tests, including the 28-test scientific
suite (576.20 seconds, two test threads). After final consent/provider/reload changes,
all four focused native catalog tests passed (106.35 seconds while overlapping the
broader suite); the final reload-availability assertion passed in a further focused
run (24.65 seconds). Final catalog/session all-target tests and doctests, Kagami/
catalog/session all-target Clippy, workspace all-target checking, formatting, diff
whitespace and docs checks passed. Catalog unit coverage includes 175 tests.
Renderer smoke passed on llvmpipe, including marker/arrow pixel checks in both
projections; it does not replace manual inspection of the new panel layout.
The existing `proc-macro-error2` future-compatibility warning remains. Full workspace
tests, worker network journeys and non-Unix execution were not rerun this increment.

The native single-template creation gap named by the preceding checkpoint is now
implemented. General compound/migration planning, catalog-file editing, headless/MCP
adoption, gesture/configuration-variable authoring and remaining X-PLUGIN runtime/
delivery gates stay open. Manual window layout QA is outstanding. No persisted
format, workload identity or kernel ABI changed; no MCP code was modified.
The overall goal remains active.

### Source-local contract and artifact lowering — 2026-09-17

Implemented `orishu.plugin-source/v2` for external plugin authors through the same
headless validate/pack/inspect authority. Explicit local contribution aliases
replace scientific requirement contracts; declared artifact aliases replace model/
integrator kernel and optional icon digests. Source v1 remains accepted unchanged
for exact inputs. V2 does not look up logical names or installed providers, build
code, execute guests or fetch dependencies. Independent external contracts remain
exact. The resulting installed-release, stored-ZIP, workload and ABI formats do
not change; equivalent exact v1 and symbolic v2 inputs produce identical bundles.

The pure shared `SourcePayload` lowerer reuses bounded declaration decoding and
ordinary exact validation/scientific hashing. Kagami owns secure source acquisition,
the explicit local graph and final release verification. Forward references are
compiled in dependency order independent of manifest order. Missing/opaque local
contracts, cycles, duplicate aliases and missing artifacts refuse without a partial
package. Unknown extension-point bytes remain opaque, not interpreted for markers.
Raw acquisition and canonical release bytes have independent aggregate budgets;
expanded exact payloads must still fit the ordinary receiving limits. There is no
recursive graph traversal or ambient inventory dependency.

Tests prove complete bundle byte equality, order and artifact-alias/path independence,
transitive rehashing after component/kernel changes, self/two-node cycle refusal,
missing/opaque providers, duplicate IDs, v1 marker refusal, bounded parsing before
an excess malformed entry, duplicate keys/nulls and expansion limits. Real CLI
validation/packing leaves an absent inventory absent, installs the resulting bundle
normally, and refuses malformed packing without publishing output or changing the
existing inventory. Fixture code is deliberately inert: these are packaging tests,
not new numerical execution evidence.

Validation passed: all plugin-crate targets, including dependency-boundary tests and
benchmark smoke; the final five source-lowering tests and plugin doctest; all 25
Kagami plugin-management tests. The final four source-management tests were rerun
after strengthening cycle and failed-publication assertions. Final Kagami/plugin
all-target Clippy, workspace all-target checking, formatting, diff whitespace and
docs checks (208 Markdown files) passed. Initial Clippy enum-size/style findings
were corrected with boxed cold payload variants and simplified guards. The existing
`proc-macro-error2` future-compatibility warning remains. Full workspace tests,
numerical/worker network journeys, non-Unix IO and window QA were not rerun; this
increment changes no widgets or renderer behavior. No MCP files were changed.

The [source format](../plugin-source-v2.md), authoring guide, ADR 0027, architecture,
context, task and roadmap now record this delivered gate. Remaining inventory crash
injection/non-Unix IO, authoring adapters and runtime/recovery/delivery gates stay
open. The overall goal remains active.

### Durable Kagami client run intent — 2026-10-09

K-RUN slice 3 is implemented under [ADR 0033](../adr/0033-persist-kagami-client-run-intent.md)
and the [run-intent format](../kagami-run-intents-v1.md). The window records each
submission (frozen bytes first, then the ledger) and each run command before it
sends it, and records the validated reply afterwards. A restart restores the
recorded operations and sends nothing. Reconcile and resubmit keep the original
identity; a restored upload sends stored bytes only after closure/root
verification. Recorded intent for another worker address is refused with the
`--host` that reconciles it. A second instance, full or read-only storage, a
damaged or newer record and an incomplete write disable submit and run commands
with a specific tooltip reason. Observation stays available.

Supporting changes: descriptor-relative file IO moved to `crate::files` with a
neutral error that keeps the OS error; `Controller::check`/`check_submit` are the
single source of action availability; the dev profile optimizes dependencies
(`opt-level = 2`, line-table debug info). Debug Wasmtime/Cranelift previously made
the real-Component Kagami suite exceed its deadlines under the default runner,
and full dependency debug info made the test build about 50 GB. A lock-retake
test race with process-spawning tests is serialized by `files::faults::serial`.

Validation: `make check` passed before this slice (2,088 tests). The slice adds
ledger, journal (including 15 SIGKILL cuts and injected failures at every write
barrier), controller and window-flow tests; Kagami all-target tests, Clippy and
the renderer smoke passed. The real-worker window journey now includes three
restart cases. Manual window verification of the tooltips is outstanding. No
worker, wire, workload or kernel format changed. Non-Unix builds disable
submission and run commands; they were not compiled locally.
