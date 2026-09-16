# Reference scientific Components

This separately locked workspace builds real kernels outside the host application
workspace. It uses the public WIT and `orishu-plugin` packet codecs, not the host
runtime or Kagami. A developer packaging example now produces installable local
vocabulary/solver bundles through the public contract. Selected workload execution
is exercised by admission tests; Kagami document authoring/export and worker
endpoint integration remain work. Installing these bundles does not enable those
unfinished application paths.

## Package and install locally

Build and wrap the independent Components as described below, then package their
exact bytes. From the repository root, the checked-in rebuilt Components can also
be used for the same reproducible demonstration:

```sh
mkdir -p /tmp/orishu-reference-packages
cargo run --locked -p orishu-runtime --example package_reference -- \
  crates/orishu-runtime/tests/fixtures/newtonian.component.wasm \
  crates/orishu-runtime/tests/fixtures/euler.component.wasm \
  /tmp/orishu-reference-packages
cargo run --locked -p kagami -- plugin install /tmp/orishu-reference-packages/vocabulary.okplugin
cargo run --locked -p kagami -- plugin install /tmp/orishu-reference-packages/solvers.okplugin
cargo run --locked -p kagami -- plugin list --all-releases
```

The packager bounds and checks both actual Component contracts without running
scientific code, builds release roots from `declarations.rs`, validates/round-trips
the stored-ZIP bundles, and creates new files only. It refuses existing destinations.
Its two outputs are not a multi-file transaction; an IO failure may leave an earlier
completed package. Use a new output directory for another build. Printed IDs come
from canonical release content, never filenames or human version labels.

`org.orishu.reference.vocabulary` defines Dynamics, source/response gravitational
mass, gravity and its channels without code. `org.orishu.reference.solvers` provides
Newtonian and symplectic Euler as separate Components, requiring those exact
scientific contracts. Solvers may be installed before vocabulary, but their
contributions are then unavailable until matching providers are enabled. Installation
does not run kernels. Pass `plugin --directory PATH` to isolate a test inventory;
normal commands default to the user's Kagami plugin store. No worker installation
is needed: selected export carries required code and scientific evidence only.

## Classical symplectic Euler

`euler` implements the Dynamics contract with one committed-state force evaluation:

```text
v_next = v + (F / inertial_mass) * dt
x_next = x + v_next * dt
```

The kernel owns this formula. It performs no field lookup, extra force evaluation,
charge/mass equivalence, speed clamp, domain clipping or implicit integrator switch.
It is first-order **classical** integration, not relativistic physics or Field CAD's
pre-force Verlet schedule. Field CAD's `fieldcad-dynamics/src/lib.rs` informed the
force/inertial-mass ownership and kick-before-drift order. Its relativistic momentum
conversion, native registry and `pinned` branch were deliberately not copied. A
relativistic model would be a distinct selected contribution; see
[migration](../../docs/migration.md).

### Kernel-owned formats

Opaque history numbers are little-endian, distinct from the
[standard scientific packets](../../docs/scientific-bulk-io.md):

| Descriptor schema | Bytes | Logical count |
| --- | --- | --- |
| `org.orishu.reference.euler.history/v1` | ASCII `OEH1`, then an entity-ID packet | Entity count |

Setup now reads the shared `orishu.simulation.instance/v1` context; validation reads
the shared `orishu.simulation.validation/v1` envelope described in
[scientific bulk IO](../../docs/scientific-bulk-io.md#instance-and-validation-envelopes).
The profile-only setup and Euler-specific validation bootstrap adapters are removed.
The context binds exact code/provider, state format, captured configuration/domain,
role mappings and bounds. This is not proof that a selected release closure was
admitted: the numerical tests deliberately use synthetic contribution pins.

Configuration uses shared `orishu.simulation.configuration/v1` deterministic CBOR,
with one dimensionless quantity property `capacity` (an integer from 1 to
1,000,000). No private `OEC1` configuration layout remains. The shared compiler
resolves declared defaults/expressions to dimensioned SI values; worker/guest
loading never consults a variable system or regenerates omitted defaults.

Euler needs no numerical look-back samples but retains explicit membership history
to reject omitted or stale entity sets. History is exactly `16 + 8 × entity_count`
bytes. Births supply a standard dynamic-entity packet, deaths a standard ID packet.
They must be disjoint, births new and deaths existing. Membership is checked before
result allocation. Load/restore never rebuilds history from current entities.
An empty set has a real 16-byte history, not missing data.

Validation refuses invalid mass/kinematics, unsupported profile and non-positive or
non-finite timestep. No universal Euler stability limit is inferred from dt alone;
there is no recommendation. Integration checks finite output and exact entity/
force/history coverage. All inputs stay immutable. Host grant, fuel and wall-time
budgets remain authoritative even if configuration permits more entities.

Work is O(entities + births + deaths) with bulk transfers, not per-particle host
calls. Initial operation-sized scratch allocations are bounded; persistent guest
scratch and host-store reuse remain hot-path gates. Handwritten code denies unsafe
Rust. Only generated canonical-ABI exports have a scoped exception for required
binding-generator trampolines.

### Build and evidence

```sh
cargo build --locked --manifest-path plugins/reference/Cargo.toml --target wasm32-unknown-unknown --release
cargo clippy --locked --manifest-path plugins/reference/Cargo.toml --target wasm32-unknown-unknown --release -- -D warnings
cargo run --locked -p orishu-runtime --example componentize -- plugins/reference/target/wasm32-unknown-unknown/release/orishu_reference_symplectic_euler.wasm /tmp/euler-new.component.wasm
```

The wrapper refuses an existing destination. Review the artifact before replacing
`crates/orishu-runtime/tests/fixtures/euler.component.wasm`. The nested lock pins
the plugin toolchain independently; JIT/native bytes never become kernel identity.

`cargo test --locked -p orishu-runtime --test reference_euler` executes the actual
Component through Wasmtime. It proves mass response, equal/opposite-force momentum,
inertial drift, constant-force first-order convergence, empty membership, births/
deaths, invalid coverage, finite-output refusal and identical next-step continuation
after restoring into a fresh engine. This is real integrator evidence, **not**
whole-workload restart, worker admission or completed X-PLUGIN.

## Direct Newtonian gravity

`newtonian` implements the Field contract independently of Euler. Field CAD's
`plugins/gravitostatics` and `fieldcad-superposition` informed point-source
superposition, explicit point exclusion and the acceleration/potential/Jacobian
conventions. No native registry, visualizer or host-side physics branch was copied.
This reference supports nonnegative point masses, not uniform-sphere interiors or
negative masses. Its one declared coupling slot supplies gravitational source and
response mass in kg; neither is inferred from inertial mass.

For each positive point source, with `r = sample - source`, it adds
`g = -G M r / |r|³`, `potential = -G M / |r|` and
`J[i,j] = -G M (δij - 3 u_i u_j) / |r|³`, with `u = r / |r|`.
The force on a dynamic responder is its independent response mass times the summed
acceleration. Self-force excludes only the identical entity ID, not all coincident
positions. Another coincident positive source or a point strictly inside the
configured exclusion radius is singular/undefined, not secretly softened.

Sources may lie outside the domain and influence its interior. A positive dynamic
response must be inside the domain; zero response produces an explicit zero force.
Initial state contains no sources and therefore samples as a natural zero field.
Advance computes field/source state from the committed **input** entity positions;
it is not recomputed from the integrator's output positions at the same boundary.
This input-kinematics phase convention must remain explicit in the model/observation
descriptor when packaging the contribution. `G` is a positive resolved SI value
(the physical fixture uses `6.67430e-11`), not a hidden host constant.

### Portable reference formats

All numeric fields are little-endian; finite values reject negative-zero encodings.
Schema names below use prefix `org.orishu.reference.newtonian.` and suffix `/v1`.

| Schema middle | Exact layout | Logical count |
| --- | --- | --- |
| `state` | `OGF1`, capacity `u32`, `G:f64`, radius `f64`, lower/upper corners, source count `u32`, four zero reserved bytes, capacity slots of `(id:u64, position:3×f64, mass:f64)` | 1 |

Configuration instead uses shared `orishu.simulation.configuration/v1` CBOR with
exactly these properties: dimensionless integer `capacity` (1–4096), positive
`gravitational-constant` (SI dimension `[3,-1,-2,0,0,0,0]`), nonnegative
`exclusion-radius` (length), and text `boundary = "isolated"`. Other boundaries,
incorrect dimensions and fractional capacities are rejected by the kernel.
Defaults belong to the selected contribution's authoring schema, not guest setup.

The shared `orishu.simulation.domain/v1` descriptor preserves lower/upper Cartesian
corners in metres and an explicit spatial scheme. This reference accepts
`continuous` only; it refuses a requested `cartesian-cells` scheme instead of
silently replacing it with direct evaluation. Physical boundary conditions are
model configuration, not a global rule silently applied to every simulated field.
The former `OGC1`/`OGD1` bootstrap input encodings are removed.

State is exactly `80 + 40 × capacity` bytes. Source IDs sort strictly; unused slots
are zero. Loading/restoring verifies the captured configuration and embedded domain
against setup context, never reruns initialization. Validation receives the shared
envelope and checks domain, role bounds, timestep and singular coupled points.
No universal stability timestep is inferred for arbitrary gravitational problems.

Sampling uses the shared [requested-channel packets](../../docs/scientific-bulk-io.md#requested-channel-sampling),
not a Newtonian-specific query or response. `OGQ1`/`OGS1` are removed. The kernel
declares acceleration (vector3), potential (scalar) and spatial Jacobian (matrix3×3)
through exact semantic schemas in `newtonian/src/channels.rs`. Context bindings
sort by slot, while queries choose any subset/order; unknown exact channels return
`ChannelUnavailable`. An observer needs these declarations, not the opaque field
state layout. Both reference kernels declare binary64 computation.

Valid samples carry quality 1 (direct evaluation). Singular/excluded points,
outside-domain points and undefined numerical results use the corresponding shared
invalidity codes, without exposing numeric zeros. Jacobian overflow can invalidate
only that channel; it must not invalidate finite forces, acceleration or potential.
Sample IDs are unique but need not be sorted. Counts are bounded by the reference
limit (4096) and setup bounds. Sampling loads an isolated guest and checks the
request's exact state/context identity. `FixedRun` now supplies bounded committed
snapshot leases; product adapters remain required.
Configuration/domain encoding now uses the generic shared boundary; its adoption
by real document selection/export is still needed
before these references become installable, authorable product plugins.

Build both workspace members with the command above; wrap
`orishu_reference_newtonian.wasm` to a new Component destination, then review and
copy it to `crates/orishu-runtime/tests/fixtures/newtonian.component.wasm`.
`cargo test --locked -p orishu-runtime --test reference_newtonian` proves analytic
force/field/potential/Jacobian values, explicit self-exclusion, distinct response
and inertial masses, validity/quality isolation, malformed state refusal and
Newtonian → stable reduction → Euler continuation after fresh-engine restore.
The composition helper exists only in tests: it is not an atomic product run owner.
The separate `fixed_run` integration suite exercises the implemented shared
[atomic owner](../../docs/runtime-fixed-run.md), including complete fresh-engine
checkpoints, late-phase rejection and detached old-snapshot sampling during advance.
`tests/admission.rs` adds separate selected-release/workload admission evidence.
It builds verified vocabulary and solver releases from `declarations.rs`, compiles
the exact required workload closure and executes it in a fresh runtime without an
installed inventory. It excludes unused code and refuses malformed captures,
graph substitutions, denied code and invalid portable state. The declaration helper
shares exact observable schemas with the guest and the packaging example; it is
not a Kagami document exporter. The admission fixture now captures field/history
using generic bounded grants: the kernels choose actual extents under host ceilings.
Different sufficient ceilings produce identical Newtonian state; no kernel-specific
sizing formula is needed in the caller. Low-level exact-layout fixtures remain
useful for wire-format regression checks. Both kernels inspect `output.is-exact`
and honor either exact extents or explicit ceilings before writing/finishing.

Direct evaluation is O(sources × responders) per advance and O(sources × points)
per query, with bounded operation-sized scratch. Persistent stores/scratch,
adaptive step-state sizing, document export and worker endpoint adoption remain
required; atomic library commit and selected library admission are implemented.
