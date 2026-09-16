# Component ABI implementation checkpoint

Status: **executable host and fixed-profile admission implemented; product/security
gates remain**. The shared
[WIT source](../crates/orishu-plugin/wit/simulation.wit) is consumed by generated
Wasmtime host bindings and independently built field and Dynamics Component fixtures. The
[accepted contract](plugin-contract-v1-draft.md) remains the full requirement;
the limitations below are not a reduction of that requirement.

## Identity and ownership

| Logical execution contract | Concrete WIT package/world | Scientific export |
| --- | --- | --- |
| `orishu:simulation/field@1` | `orishu:simulation@1.0.0`, world `field` | `orishu:simulation/field-kernel@1.0.0` |
| `orishu:simulation/dynamics@1` | Same package, world `dynamics` | `orishu:simulation/dynamics-kernel@1.0.0` |

Both export `common` lifecycle operations. Imports are the closed `buffers`
interface and the shared, type-only `types` interface. No WASI, logging, filesystem,
network, clock, random or native-code API is linked. Independent kernels remain
independent Component artifacts. Type checking/import resolution happen before
instantiation; digest checks and a byte ceiling precede Component compilation.

`crates/orishu-runtime` owns the engine; it is intended for both Kagami and the
worker. It owns no document authority, installer, membership state or network IO.
The pure plugin crate contains WIT but acquires no engine dependency. Wasmtime
48.0.2 is pinned in the runtime only, with minimal synchronous Component Model/
Cranelift features and no on-disk code cache. JIT output is never workload identity.

Existing workload/graph/lifecycle strings and canonical bytes have **not** been
silently reinterpreted. These worlds are bound through the explicit v3 selected
scientific profile and independent shared admission. Workers still do not
advertise a scientific execution capability based on this foundation alone.

## Implemented host behavior

The WIT explicitly separates setup/load/validation/checkpoint/restore/close, field
initialization/advance/sampling, and Dynamics history/membership/integration.
Field initialization has no entity parameter; Dynamics history initialization
explicitly receives initial entities. Guest-local session tokens are never
persisted as scientific state. Guest rejection enums and numerical advice have
fixed-size results rather than unbounded guest diagnostic strings.

`HostState` lends immutable input bytes and isolated writable candidates through
typed Component resources. Grants bind to an invocation and have a versioned
schema label, byte extent and logical value count. Exact grants require the full
declared extent. Explicit bounded grants expose independent byte/value ceilings
and accept a kernel-chosen actual extent. `output.is-exact` distinguishes them;
`describe` retains the original extent/ceilings for that grant. Writes are contiguous;
finish requires coverage of exactly the reported actual length and is accepted once. Invalid output
access/finish poisons the candidate even if guest code catches the returned error.
Taking output does not scientifically validate it or commit a run boundary.
Old grants are revoked together, and borrowed-resource ownership is enforced by
the Component canonical ABI plus the host resource table.

`InitializeBounded` and `InitializeHistoryBounded` use these grants for natural
field/history capture. Callers supply a schema and host ceilings, never a
kernel-specific sizing formula. Capacity is charged in full before guest execution;
storage grows geometrically only for checked written prefixes, up to the ceiling.
Returned buffers contain only written bytes and the actual logical count, without
padding to capacity. Both zero bytes/count and independently smaller extents are
allowed; scientific schema validation remains separate. Exact packet grants are
unchanged. The selected-workload admission suite now uses this generic capture
path with actual Newtonian/Euler kernels.

The new `output.is-exact` import is an additive development-WIT capability. Older
Components using the unchanged exact methods still type-check; newer Components
requiring this import are rejected by an older host lacking it. No adapter guesses
the policy. Both reference Components were rebuilt and have new artifact identities.
This does not change existing v2 workload identities or portable reference state
formats. Runtime advancement/checkpoint extents remain fixed for now; bounded
initialization alone does not implement adaptive state or membership scheduling.

`Sandbox::compile` checks exact Component bytes separately, without guest execution.
`invoke_field` creates an isolated guest for initialization, loaded-state validation,
field advance, snapshot sampling, checkpoint or restore. Every path performs setup
and requires successful close before returning. Load/restore never calls natural
initialization. Advance loads explicit prior state and validates relevant inputs
before returning **both** finished field and force candidates. A late failure
discards both. The `initialize_field` convenience method uses this same path.
Timestep advice is bounded, positive and finite and never rewrites authored `dt`.
Guest rejections retain typed codes. These are raw lifecycle operations, not a
scientific run owner or validated workload admission.

The shared [scientific bulk IO codecs](scientific-bulk-io.md) now define bounded
Dynamics, per-slot coupling and force projections. `Buffer::scientific_batch`
validates standard descriptor/schema/count agreement. The reusable `ForceReducer`
checks exact field/response coverage and stable-order finite sums. These pieces
are now wired into the single-partition `FixedRun` owner. Real reference Euler and
Newtonian Components now consume them through shared bounded `InstanceContext` and
`ValidationInputs` envelopes. The `invoke_*_bound` methods check exact artifact,
configuration/domain, state-format and validated-step input agreement before
guest execution. Raw `invoke_*` methods remain lower-level ABI test surfaces, not
workload admission. See [envelope details](scientific-bulk-io.md#instance-and-validation-envelopes).

`invoke_dynamics` uses the same engine, resource interfaces, shared WIT types,
numerical-advice checks and interruption policy. It supports history initialization,
loaded-history validation, integration, birth/death history transitions, checkpoint
and restore. Integration requires both entity and history outputs before returning.
History is always explicit; missing entries are not automatically initialized, and
load/restore never invokes `initialize-history`. This operation layer does not
schedule births or decide their first force-evaluation boundary: that remains the
fixed-profile coordinator's job.

Sampling loads an immutable snapshot in a separate guest; guest-private mutation,
trap or cancellation cannot change another operation's instance or committed bytes.
This bootstrap path creates a store/candidate allocations per operation. Reusable
supervised step storage is still required before hot-path/product adoption.

Initial explicit ceilings: 32 MiB component bytes; one 256 MiB guest memory;
eight tables with at most 4,096 elements each; sixteen core instances; 512 KiB
guest stack; 10 million fuel units per complete operation including setup;
64 grants/128 MiB total granted bytes; 1 MiB reads and 64 KiB physical write frames;
4,096 host calls and
512 MiB aggregate transferred bytes per invocation. Caller policy may be stricter.
Each write carries a logical prefix length; padding is ignored as data but charged
as physical transfer work. WIT's fixed-length `list<u8, 65536>` makes canonical
lifting bounded **before** the host callback. Invalid prefix lengths poison output.
The host checks read lengths before creating return buffers. The development WIT
and fixture were regenerated together; old dynamic-write Component types fail
conformance rather than being silently adapted.

Fuel is instruction work, not a wall-clock timeout or scientific timestep. A
separate five-second default operation deadline covers instantiation/start, setup,
load/validation, the requested phase and close. Caller deadlines can shorten it.
An engine epoch heartbeat checks independent cancellation/deadline tokens; host
calls and candidate extraction check the same token. Cancellation does not cancel
other stores sharing the engine. This does **not** bound synchronous compilation.
An operation can additionally link one monotonic host `Cancellation` lifetime.
Policy-shortened operations retain both their own token and that parent, and a
second parent cannot silently replace the first. Cancelling a child leaves its
siblings and parent alive; revoking the parent cancels all linked guest operations.
The worker admission adapter uses this for its formation execution lease. This
host-only control addition changes neither WIT nor scientific identity, and is
not an atomic publication guard or a way to interrupt native JIT compilation.

## Evidence and remaining security gates

The checked-in `field.component.wasm` is built from
[fixture source](../crates/orishu-runtime/tests/guest/src/lib.rs) using its separate
locked plugin toolchain. It is **not a physics implementation**. Tests execute it
through Wasmtime and prove nonzero initialization output, unchanged immutable
inputs, digest/world/import/size/memory rejection, an actual `OutOfFuel` trap,
intentional trap, missing/partial finish and invalid write rejection. The fixture
also exercises complete field lifecycle ordering, explicit-state checkpoint/restore,
successful advice and numerical rejection, invalid advice, incomplete forces after
finished field output, close failure, and sampling traps after finished output.
A mode that loops in `initialize` still succeeds through every loaded-state path,
proving those paths do not regenerate natural defaults. Separate tests execute a
deadline-interrupted loop and independently cancelled concurrent work on one engine.
Focused grant tests cover work bounds, write padding/physical-frame accounting,
revocation, duplicate finish and candidate extraction. These are ABI/security
proofs, not numerical gravity or full checkpoint/restart equivalence evidence.

The separately compiled `dynamics.component.wasm` uses a deliberately small
[history fixture](../crates/orishu-runtime/tests/guest/dynamics/src/lib.rs): sorted
byte entity IDs and explicit per-entity counters, not Euler or another physical
integrator. Its tests prove initial/empty history, one integration, birth/death
history changes, preserved surviving counters, and identical next-step output
after checkpoint/restore. They also reject missing or malformed history, duplicate
births, unknown deaths, incompatible roles, validation/close failures, traps and
completed entity output with missing history. The counter schema is **test-only**;
it is not the product's scientific entity/history wire format.

The independently compiled [classical symplectic Euler contribution](../plugins/reference/README.md)
adds real numerical evidence through `euler.component.wasm`: inertial-mass response,
equal/opposite-force momentum, inertial drift, first-order constant-force convergence,
explicit membership and identical next-step output after restoring into a fresh
engine. It does not claim relativistic physics, a field solver or full-run restart.
The separate Newtonian Component adds direct acceleration, potential and Jacobian
sampling, independent source/response mass, explicit self-force exclusion and
coupled force-reduction/Euler restart evidence. Optional Jacobian overflow does not
poison finite force or other sample channels. Both reference Components now use the
shared setup/validation envelopes; neither is yet an installable/admitted release.
Configuration/domain now use shared bounded CBOR inputs and dimensioned resolved
values; a pure helper compiles declared expressions/defaults through the shared
variables engine. Generic requested-channel sampling now binds exact context/state,
channel schema/shape/dimension and precision, validates flat ranges/quality/validity,
and supports subsets/order/unavailability through the actual Newtonian guest.
Document/workload version integration and actual authoring/export remain work.

The [single-partition fixed run owner](runtime-fixed-run.md) now validates captured
state, executes the real field/reduction/integrator pipeline, validates all candidates
and atomically publishes complete boundaries. Complete in-memory portable checkpoints,
fresh-engine restore and bounded detached committed-field leases have real-kernel
evidence. The shared `admit` path now constructs that owner from independently
verified [v3 selected scientific workloads](workload-v3.md), compiles actual
Components and validates captured state without initialization. Real release
evidence is covered separately from synthetic low-level fixtures. This is not a
worker API implementation.

These hardening and delivery gaps remain mandatory for product integration:

- Account for lifted frames, output extraction copies, compiler resources and all
  concurrent instances in an aggregate owner budget. Bound compilation work as
  well as submitted artifact bytes; execution deadlines do not interrupt the JIT.
- Complete bounded, structured failure attribution by workload/run/instance/phase/
  partition, beyond the implemented typed guest-rejection and interruption codes.
- Complete initial-state/schema/dimension/admissibility validation and explicit
  output sizing for variable opaque state. Raw `Buffer` values are not admitted
  scientific projections; their metadata must be bound by the supervisor to
  exact workload/run/field/partition/boundary identities and selected contracts.
- Replace the fixed owner's disposable stores/projection allocations with reusable
  bounded storage; integrate membership/emitter lifecycle and variable output sizing.
  Bind sampling declarations to independently admitted observables and product adapters.
- Connect authoring capture/export and worker admission to the implemented shared
  selected-workload admission path, including delivery and fenced run allocation.
- Complete adversarial/conformance coverage, including malformed canonical
  arguments, compilation/resource bombs, unauthorized nested
  imports, retained guest handles, quality-bearing sampling and every phase failure.

Rebuild the external guest with `cargo build --locked --manifest-path
crates/orishu-runtime/tests/guest/Cargo.toml --workspace --target wasm32-unknown-unknown
--release`, then use `cargo run --locked -p orishu-runtime --example componentize
-- <core-module> <new-component-path>`. The helper wraps metadata without executing
code; it is developer tooling, not workload admission. Review fixture byte changes.
