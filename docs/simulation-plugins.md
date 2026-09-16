# Simulation plugins

Accepted concrete specification (2026-09-16): [X-PLUGIN v1](plugin-contract-v1-draft.md).
That document records accepted decisions here and in ADR 0027; acceptance does
not mean all runtime/wire implementation has been delivered.

This document describes the complete target contract. Initial Unix source/package
and CLI management are now implemented; see [local tooling](plugin-authoring-tools.md)
for exact delivered scope and remaining gaps.
[ADR 0027](adr/0027-plugin-contributions-and-immutable-releases.md) records accepted
decisions and alternatives; [X-PLUGIN](tasks/define-and-implement-plugin-contract.md)
tracks remaining design gates and implementation slices.

Orishu Kagami is extensible at the level of scientific models. Kagami will ship
with a useful initial set of simulation plugins, including gravity and
electrodynamics, while advanced users can develop, install, inspect, update,
and remove additional plugins for other fields and numerical methods.

A **simulation plugin** is the product-facing package that makes a physical
model authorable in Kagami and executable by Orishu. Plugin development is a
software-development activity performed in an IDE, build system, or coding
agent—not inside Kagami's CAD interface.

## The three product roles

| Role | Primary tools | Responsibility |
| --- | --- | --- |
| Cluster operator | `orishu-worker`, `orishuctl`, `orishu-monitor` | Provision and operate the compute platform, form clusters, secure them, and keep them healthy. |
| Researcher | Kagami | Author experiments, select installed simulation plugins, submit immutable workloads, and inspect live or recorded runs. |
| Simulation plugin developer | IDE, compiler, tests, packaging tools | Implement and verify new models or numerical methods and package their authoring contract and executable component for researchers. |

These are roles rather than mutually exclusive accounts. One advanced
researcher or agent may develop a plugin, install it in Kagami, author an
experiment with it, and submit that experiment to a cluster. The permissions
and tools used at each boundary remain distinct.

## What a plugin contains

The admitted plugin profile contains data and sandboxed executable artifacts,
not native host extensions:

- a versioned plugin manifest with stable plugin and compatibility identity;
- declarative authoring schemas describing the models, fields, parameters,
  dimensions, constraints, initial conditions, object components, their state
  ownership and composition constraints, phase/channel contracts, placement
  compatibility, and observations Kagami can offer;
- where computation is contributed, digest-pinned WebAssembly workload component code implementing Orishu's
  versioned, capability-limited component lifecycle;
- any other digest-addressed schemas or runtime inputs required by that
  component; and
- optional documentation, examples, and experiment starters that do not alter
  execution semantics.

The public package, the executable, and its internal implementation are
different concepts:

| Term | Meaning |
| --- | --- |
| Simulation plugin | Installable product capability presented to a researcher. |
| Kernel | Independently compiled scientific executable implementing one Orishu-owned execution contract. |
| Kernel instance | One configured use of a kernel in the workload graph. |
| WebAssembly Component | Binary technology used to deliver and sandbox a kernel. |

A kernel may use shared algorithms and libraries internally. Each compiled
kernel remains independent; a plugin may bundle multiple kernels. Installing a
plugin does not grant its code ambient access to Kagami or a worker process.

Orishu owns execution contracts such as field update and dynamics integration.
These are distinct from plugin-owned scientific contracts such as the gravity
field's vocabulary. A gravity plugin may contribute that vocabulary and two
separate kernels implementing the field-update contract for gravity: classical
and GEM. Selecting one includes only its executable and required dependencies.
This is our execution/packaging rule, not a limitation of Wasm itself. Common
lifecycle functions and multiple phases do not constitute multiple scientific
implementations. Concrete role interfaces remain X-PLUGIN/O-WASM design work.

Older prose and current code/wire formats call a kernel a `workload component`
and a kernel instance a `component instance`. Those names remain compatibility
references until an explicit API/format migration; they do not define another
layer of scientific executables.

A vocabulary-only plugin need not contain executable code. Another independently
developed plugin may supply a model implementing that exact scientific contract.
The manifest bundles contributions, not a mandatory schema-and-kernel pair.

## Shared contract ownership

The MVP will introduce a dependency-light `orishu-plugin` crate for shared
identities, contribution envelopes and typed payloads, canonical codecs and pure
bounded validation/resolution. Archive, filesystem, network and CLI operations
remain in Kagami's shell. Workers consume selected scientific contracts through
workloads; they do not install plugin bundles.

Variables, catalog, workload and plugins are coupled parts of one Orishu–Kagami
contract. Audit existing type ownership and preserve an acyclic dependency graph
before implementation; the crate names do not settle every type's placement.
Revisit this layout after MVP if actual consumers justify it. Concrete wire
schemas, codecs and migration policy remain X-PLUGIN design gates.

## Extension points and contributions

Exported dimensioned constants are an explicit declarative contribution, consumed
through the shared variables engine. Binding/import captures exact provider and
scientific values in the experiment/workload; updates do not change existing
expressions through a live installed namespace. Collisions use provider resolution,
not last-registration-wins. The concrete proposal is in the v1 review draft.

Orishu Kagami owns a versioned set of **plugin extension points** in its shared
plugin contract. An extension point names a kind of declarative contribution,
not a directory a bundle may overwrite. Initial examples include entity
component schemas, field-family contracts, computational models, observation
channels, exported constants, and workload component-plan fragments.

A plugin release registers one or more **contributions** at those extension
points. One bundle may therefore supply an object component, a field-family
contract, two computational models, and their executable workload components.
Extension-point policy determines how the contributions compose: entity
components and model choices accumulate, while an experiment selects exactly
one compatible computational model for each active field family.

Contribution identity is provider-qualified by plugin release, extension
point, and plugin-local contribution identity. Display and scientific names may
collide; Kagami shows the provider and never lets installation order select one.
For a required exact contract, reuse an existing experiment provider pin. An
unbound dependency with one eligible provider resolves automatically; several
eligible providers require an interactive choice. Persist the resulting selection.
Headless authoring returns that ambiguity as a structured protocol outcome with
eligible choices. The caller submits an explicit selection and retries; it does
not need a terminal prompt. Interactive Kagami renders the same outcome as a
choice. Returned options are bounded and retry validation uses current state.
Export requires resolution to be complete: Orishu verifies the pinned closure,
never asks for or chooses a provider. Fetching missing bytes by exact digest is
artifact delivery, not dependency-provider selection.
Independently developed plugins cooperate through exact **scientific contract
identities** consisting of a stable name, version, and canonical contract
digest. A model may be installed before the field-family contract it implements;
that contribution remains unavailable until its bounded dependency closure is
satisfied, while unrelated contributions from the same valid release remain
available.

The common contribution envelope is stable, while each Orishu-Kagami-owned
extension point has its own typed, versioned payload. Unknown extension points
and missing or incompatible dependencies produce structured contribution-level
availability diagnostics. An unsupported contribution and its declared
dependents remain dormant while understood independent contributions from the
same valid release may activate. Plugins cannot create new host extension
points or make arbitrary payloads authoritative merely by naming them.

The initial extension-point set is scientific rather than visual. Contribution
schemas may carry bounded presentation annotations such as labels, grouping,
descriptions, preferred display units, icons, and hints selecting a host-owned
generic editor or visualizer. Kagami owns every menu, window, widget, renderer,
command binding, and presentation lifecycle. Plugin-contributed views, windows,
renderers, widgets, and executable UI are outside this contract and require a
separate architecture and security decision if later evidence justifies them.

## Field-to-entity execution

All selected field and integrator kernels validate numerical timestep admissibility
against supplied discretization, configuration and profile before execution.
Positive finite `dt` is necessary but not sufficient. Advice such as a stability
upper bound is reported, never silently applied to authored time controls. A
rejection does not advance simulation time or publish candidate state.

Numerical history covers emitted entities and removals, with explicit cold-start
semantics and boundary identity. Checkpoint membership, required history and
emitter accumulators/random-stream state together. Missing required history is
not an implicit zero. Coordinate birth/death scheduling with X-EMITTER and prove
spawn/checkpoint/restore/next-step equivalence.

The first pass admits only integrators compatible with one field/force stage
followed by integration using current entity state, accumulated forces, declared
bounded history and `dt`. Pre-force/post-force integrator hooks and repeated
force evaluations within a tick are deferred for review once field and Dynamics
kernels execute together. Such support requires compatible field kernels too;
it is not implied by selecting an integrator. See the
[integration follow-up](tasks/kagami/define-composed-object-execution.md#follow-up--staged-integrator-capabilities).

Field CAD's reported per-entity circular history of positions and velocities is
a useful reference for trajectory rendering and smoothing. Distinguish numerical
history required by an integrator from observer trajectory history: required
history is bounded scientific state, validated and checkpointed for exact resume;
display history may have different retention and must not control the solve.
Storage reuse is possible only while preserving those ownership/lifetime rules.

The user selects a plugin-contributed dynamics-integrator kernel; its numerical
formula is not owned or hard-coded by Orishu. Euler and Verlet are Field CAD
examples, and other methods such as RK4 are extensibility targets. The shared
contract defines inputs, outputs, history and scheduling compatibility, not the
formula. A method requiring extra force evaluations cannot infer them from one
accumulated force vector; support for such methods must be explicit in the
execution profile rather than assumed from the pluggable-kernel interface.

The Field CAD target flow clarified on 2026-09-13 uses entity/component
composition: an entity has identity and attached data components; systems/kernels
operate on matching entities. Attaching components in authoring selects behavior,
not arbitrary executable systems or live edits to an accepted run. This is an ECS
domain model, not a mandate for an ECS library or persisted memory layout.

A field is plugin-defined scalar/vector/multi-channel state over the authored
domain, with an admitted bounded numerical representation. The author selects
the field and its kernel. A field-coupling component makes an entity participate
in that field's computation; for Newtonian gravity, gravitational mass can
describe both its source strength and response coupling. It remains distinct
from Dynamics' inertial mass.

For the initial independent-field flow, Orishu supplies each field kernel with
its own previous state and a read-only projection of the matching entities:
stable identity, required kinematics and declared coupling properties. The same
entity may appear in several projections. Kernels do not read other fields or
observe one another's partial entity updates. The field kernel both computes its
candidate field state and emits per-entity force contributions. This does not
require a separately packaged coupling kernel in this initial profile.

The dynamics-integrator kernel receives the entity state and the admitted force
contributions, reduces them in the declared deterministic order and integrates
entities with Dynamics. Field kernels never write authoritative entity position
or velocity. Entities coupled to a field but lacking Dynamics remain fixed, as
already required by ADR 0020. Orishu commits field and entity candidate state
together only after all required work and validation succeed.

The first pass uses a fixed two-stage pipeline, not a configurable execution
schedule: (1) compute all fields and accumulate force contributions using the
same committed entity boundary, then (2) reduce the complete contributions and
integrate dynamic entities once. No field sees positions partly updated by
another field. Independent field work may execute concurrently; completion order
does not determine floating-point reduction order. Validation and atomic commit
follow the two scientific stages. An empty force set still permits inertial motion.

Conceptual signatures, **not frozen ABI**:

```text
field.update(previous_field, coupled_entities, step_context)
    -> (candidate_field, entity_force_contributions)
dynamics.update(dynamic_entities, force_contributions, step_context)
    -> candidate_dynamic_entities
```

These are bounded bulk inputs/outputs, not per-entity host callbacks. Partition
context and required source/halo data must preserve the same scientific contract
when distributed; matching entities need not mean replicating the entire scene
on every worker.

Field `init` constructs the kernel-defined natural default state for the domain
and performs the kernel's bounded setup for upcoming computation. A zero-filled
matrix is one possible default, not a host requirement. It does not take coupled
entities, inspect the rest of the scene, advance time or move objects. Given the
same final authored values, creating a field before or after the entities must
not change the experiment's initial conditions. The completed collection of
authored fields and entities is the initial setup; the first update receives the
coupled entities and computes the field and forces.

Validation of that completed setup is separate. A kernel's construction default
is not a promise of scientific admissibility with every later source/configuration;
incompatible initial conditions produce diagnostics rather than silent repair.
`init` is not a source-consistency solve. Kernel setup remains sandboxed, bounded
and restricted to declared capabilities; “setup” grants no ambient host access.
State needed for reproducible execution or checkpoint restoration cannot hide in
untracked setup side effects. Existing lifecycle initialization/load/restore calls
must be mapped explicitly to these semantics, not renamed by implication.

Still to specify: the model's precise force evaluation time versus returned field
state and its compatibility with the selected integration method. The two-stage
orchestration does not itself choose an Euler variant or temporal staggering.
Default construction executes through the local runtime's sandbox when Kagami creates a field.
Its scientific output is captured in the experiment as authored initial state.
Reopening and workload submission load/export that captured state; they do not
regenerate it by running default construction again. Runtime-only setup is
reconstructed separately without changing captured scientific values. State
needed scientifically for restart belongs in the explicit state/checkpoint
contract, not an assumed disposable cache.

Still specify exact domain/configuration inputs, serialization/storage of the
captured field state, and concrete exports for default construction versus
runtime setup/load/restore. These are not permission to add coupled entities
back to `init`. Field creation uses the document authority: sandbox failures or
invalid output cannot publish a partially created field. Packaging/inspection
commands remain non-executing; explicit field creation is a different operation.

Reinitialization is a normal authoring operation, not exceptional repair. The
scene inspector offers **Reinitialize field** for the selected field, invoking
its pinned kernel's default construction with current domain/configuration.
Changes to a field's domain or compute parameters also reinitialize its captured
state as part of that edit. Presentation-only edits do not. The UI makes this
effect clear; there is no silent resampling or reset on reopen/submission.

The document authority atomically accepts the edited settings and all affected
new field states as one undoable revision, or rejects the whole operation if
construction/validation fails. Shared-domain edits cover every affected field.
Undo/redo restore the captured before/after states rather than rerunning kernels.
UI and MCP use the same commands and outcomes. These are authoring actions,
never mutations of an active run or replay stream.
ADR 0024's graph remains a representation for the fixed pipeline; configurable
pipelines and coupled/multi-stage schedules are future extensions, not first-pass
requirements. Their eventual admission requires an explicit versioned profile.

## Field storage and typed sampling

Successful sampling carries quality (direct evaluation, interpolation or
reconstruction) separately from numeric precision and invalidity. Concrete flags
remain part of draft review. Use flat bounded output grants and validity/quality
arrays, not an allocation per sample cell. Reuse is allowed only after readers
release a buffer; cache keys include snapshot and complete query identity.

Kagami consumes a common runtime interface rather than orchestrating kernels or
peer buffers itself. Its local implementation embeds the same execution engine
used by one Orishu worker, without requiring cluster formation. Its cluster-proxy
implementation submits workloads and queries/observes the cluster; it does not
independently step scientific state or coordinate peers. Kernel invocation,
buffer management and sampling are runtime responsibilities.

The local runtime is also available during authoring when execution targets a
cluster. Field creation/reinitialization requests use the installed pinned kernel
through this local runtime, then return scientific state to the document
authority for validation and atomic capture. Authoring does not require a live
cluster. Initialization cannot mutate an active local run: invocation state and
resource ownership remain isolated. Concrete runtime API and embedding details
remain specification/implementation work, not delivered behavior.

Field state is opaque to Kagami and to the runtime's numerical interpretation.
The selected kernel owns its meaning and representation; the host owns buffer
allocation, lifetime, transfer and access grants. There is no requirement to
normalize solver state into a host-defined dense grid. The public scientific
field contract instead declares observable channels, value shapes, dimensions
and units, and the kernel supplies bounded typed sampling.

Observable values use named channels with stable scientific identities, declared
value shapes, physical dimensions/canonical SI units and coordinate-frame
semantics where applicable. Electromagnetic observation is not limited to E/B:
depending on the selected kernel, channels may also expose electric scalar
potential, vector potential and field Jacobians. The Field CAD experience makes
kernel-provided Jacobians a required design case for flow-line consumers, not a
reason for Kagami to decode private solver state. These are sampled quantities,
not separate geometry or arbitrary derivative-operation APIs.

Initial channel value shapes are scalars, fixed-size vectors and fixed-size
matrices. A Jacobian is a matrix-valued channel, not disconnected scalar channels.
Its component/derivative-axis ordering, coordinate frame and physical dimensions
must be explicit. Sizes are fixed by the channel declaration and bounded before
allocation. Concrete wire ordering, numeric precision and maximum sizes remain
to be specified; supporting matrices is no longer an open design choice.
Channel support must describe the selected kernel, not promise that every model
of a field family supplies all possible observables. Potential conventions and
derivative meaning belong in the scientific channel contract, not display labels.

Each selected kernel explicitly declares its supplied channels by exact scientific
contract reference. Kagami offers those channels for probes. If an otherwise valid
model switch removes a requested channel, preserve the probe and its exact channel
request but mark that channel unavailable with a structured reason. Do not silently
delete, rebind by name or substitute zeroes. Other supported probe channels remain
usable; restoring a compatible provider can restore availability without rebuilding
the probe. Channel declaration syntax remains specification work, not this policy.

The required initial sampling operation is batched point sampling. Probes may
have a single point, finite plane, sphere, box or cylinder geometry with
user-defined sampling density/count. Geometry defines a collection of spatial
sample points; it does not add a shape-specific kernel interface. Observation
instrument processing generates bounded point batches and consumes their typed
results through the runtime interface. Kernel sampling receives positions, not
widgets or geometry-specific rendering requests. Specialized integral, gradient
or aggregate measurement operations are deferred from the initial kernel ABI.
This does not exclude sampling a Jacobian/derivative channel already supplied by
the selected kernel through the ordinary point-sampling interface.

Point generation belongs to the observer. Kagami is the current observer
application; future observers may generate their own points through the same
public runtime sampling interface. This is not a single-observer restriction.
Surface and volume sampling are supported instrument modes for sphere, box and
cylinder probes. Their precise placement, ordering, density/count units and
generation limits belong to K8/K-OBSERVATION, not X-PLUGIN contract gates.
Attached geometry uses the object's transform at the queried boundary. Split
batches retain point/result correspondence and one snapshot identity; point-count
overflow or excessive density is rejected before allocation/work. Batching does
not permit mixing readings from different simulation boundaries.

Conceptually, not frozen ABI:

```text
sample(field_snapshot, channel_ids, positions[], query_context)
    -> typed_samples_with_validity[]
```

Requests identify an exact authored initial or committed snapshot and bounded
positions/channels. Results carry scientific identity, boundary/provenance and
validity, including out-of-domain or undefined values. Kagami's generic probes,
vectors, flow lines and MCP consume these typed values; kernels do not contribute
widgets or rendering code. Sampling is read-only and outside scientific commit,
with isolated bounded execution: failures or observer pressure cannot mutate
scientific state or block a step. Full-domain observation is an explicitly
described sampling/observation projection, not a promise that opaque checkpoint
bytes are directly renderable or that finite samples reconstruct all solver state.

The host may retain and reuse buffer storage and transfer opaque payloads to
other instances of the same pinned kernel under the declared compatible state/
exchange schema. Matching kernel code alone does not validate workload, field
instance, boundary, partition, schema or coverage: those remain checked metadata.
The host need not interpret numerical contents, but must enforce sizes, bounds,
ownership, compatibility, integrity and invocation access. Opaque payloads need
an explicit transferable representation; they cannot silently contain process-
local pointers or an assumed Rust memory layout.

Buffer reuse and avoiding redundant staging copies are implementation goals,
not a guarantee that a WebAssembly Component ABI maps arbitrary host memory
without copying. Keep capability-scoped bulk resources and a safe bounded-copy
path; any mapped/zero-copy path must preserve isolation and explicit lifetimes.
Committed inputs remain immutable while kernels write isolated candidate
outputs, and retained observer snapshots cannot be overwritten on reuse.

A recorded opaque snapshot needs the pinned sampling implementation and required
artifacts to answer new sensor queries. Recorded sample-only observations can
replay those measurements, but cannot promise arbitrary retrospective probes.
Concrete sampling ABI, spatial routing/exchange profile, snapshot retention and
recording capabilities remain specification work; opaque storage does not settle
those mechanisms implicitly.

## Packaging and release identity

MVP distribution is local-bundle-only. Plugin registry, discovery and remote
download/update services are post-launch development; publisher signatures are
outside the initial delivery scope. Local origin does not make code trusted:
manifest/artifact validation, digest checks, bounds and sandboxing still apply.
Updating locally installs an explicitly supplied new bundle and changes the
new-authoring default without replacing existing pinned releases. Concrete local
command syntax remains to be finalized; no automatic network lookup is implied.

The user-facing distribution is one isolated, manifest-driven plugin bundle.
It may contain schemas, WebAssembly Components, documentation, examples, and
assets, but no file registers anything merely by occupying a path. Releases
never overlay a shared union filesystem, and uninstalling one cannot reveal or
replace files from another.

Plugin authors maintain an ergonomic source manifest with logical identities,
contributions, dependencies, compatibility declarations, and paths to build
outputs. They do not calculate digests, sizes, or the plugin release identity
by hand. A headless packaging command:

1. parses and bounds the source manifest;
2. validates every known extension-point payload;
3. inspects referenced schemas and WebAssembly Components;
4. streams and records every artifact's digest, exact size, and media type;
5. constructs and canonically encodes the complete typed release manifest;
6. hashes those canonical bytes to derive the plugin release identity; and
7. assembles the isolated bundle and optional signature material.

The release identity is not a field inside the canonical bytes it hashes. Full
contribution identities are derived afterwards from the release identity,
extension point, and plugin-local identity. Installation independently repeats
the manifest and artifact checks and recomputes the release identity before
atomically admitting anything.

Archive encoding, compression, entry order, and filename are distribution
details. Repacking the same canonical manifest and artifact closure preserves
the release identity. The exact archive container, publisher-signature policy,
and remote update-source protocol remain future distribution decisions; only
the local container/profile blocks MVP specification.

## Planned Kagami plugin commands

Plugin commands use the normal `kagami` executable in headless mode; they do
not launch the authoring window.

| Command | Intended result |
| --- | --- |
| `kagami plugin validate <source-or-bundle>` | Perform bounded manifest, contribution, dependency, artifact, and component-contract checks without installing. |
| `kagami plugin pack <source> [--output <bundle>]` | Build a self-contained bundle, derive its release identity, and report its contributions and artifacts. |
| `kagami plugin inspect <bundle-or-installed-release>` | Show logical/release identity, origin, contributions, dependencies, artifact digests, compatibility, and diagnostics. |
| `kagami plugin install <bundle>` | Validate and atomically add an immutable release without replacing existing releases or editing experiments. |
| `kagami plugin list [--all-releases]` | List logical plugins, installed releases, default release, enablement, origins, and contribution availability. |
| `kagami plugin update <plugin-id> <bundle>` (draft syntax) | Validate and install an explicitly supplied local release and make it the default for new authoring. Registry discovery and remote updates are post-launch. |
| `kagami plugin set-default <plugin-id>@<release-id>` | Select an installed release for new authoring without migrating existing experiments. |
| `kagami plugin enable <plugin-id>` | Persistently allow the logical plugin's contributions to participate in availability resolution. |
| `kagami plugin disable <plugin-id>` | Persistently suppress all releases of the logical plugin without removing files or changing experiments. |
| `kagami plugin remove <plugin-id>@<release-id>` | Remove one exact installed release after explicit checks and warnings; references in experiment files are never rewritten. |
| `kagami --enable-plugin <plugin-id>` / `--disable-plugin <plugin-id>` | Apply a process-only override over persistent enablement for this Kagami invocation. |

The command list is the planned user contract, not a claim that the current
binary implements it. UI and future MCP plugin management must command the same
inventory authority and report the same identities, states, and diagnostics.

## Development-to-execution flow

```text
IDE / build and test tools
        |
        | build and package
        v
simulation plugin
  manifest + authoring schemas + digest-pinned component code
        |
        | install and validate
        v
Kagami plugin inventory
        |
        | researcher selects model and authors parameters/initial state
        v
experiment revision
        |
        | compile and pin the required plugin artifacts
        v
immutable workload closure
        |
        | submit, independently validate, execute
        v
Orishu run and observations
```

Kagami uses the declarative schema to construct authoring controls, validate
dimensions and expressions, and explain unavailable or incompatible features.
It does not compile source code or provide a general code editor. Local preview
uses the same workload lifecycle and sandbox assumptions as remote execution.

Workload compilation never submits “use whatever plugin named gravity is
installed.” It records the exact model/schema identities and includes
digest-pinned component and input descriptors in the workload closure. Orishu
validates those bytes, lifecycle compatibility, capabilities, resource bounds,
and workload schema independently of Kagami before accepting them.

Compilation starts from the provider-qualified contributions the experiment
actually selects or references and follows only their bounded transitive
scientific-contract and artifact dependencies. It does not copy a complete
plugin bundle. If an object component comes from plugin A while the selected
field-update model comes from plugin B, the workload includes those two
contributions and what they require; an unselected kernel shipped by A is
neither needed nor authorized to enter the workload. Documentation, examples,
icons, and unrelated authoring contributions likewise stay in Kagami.

Compilation creates configured kernel instances and a bounded deterministic
step plan. Orishu hosts those instances, mediates their typed channels and may
place independently partitionable work on different eligible nodes. Plugins do
not call one another or share ambient memory. A kernel may internally reuse
algorithms and libraries, but composition between independent kernels
follows [ADR 0024](./adr/0024-orishu-orchestrates-a-workload-component-graph.md).

## Field families and computational models

A field family names physical state conceptually defined at every point of an
authored domain. A computational model is a plugin-contributed schema plus
executable update method for that family; its cells, mesh, basis, particles, or
other representation are model details.

An experiment selects exactly one model for each active family. Coulomb and
Maxwell/Yee are alternative electromagnetic models that share stable object
coupling through electric charge but cannot simultaneously own the same field.
Classical gravity and GEM are analogous alternatives sharing gravitational
coupling. Compatible families may coexist, and hydrodynamic plugins may model
a real medium and flow as field state. Model incompatibility is validated
explicitly and is never resolved by plugin installation order. See
[ADR 0023](./adr/0023-fields-are-plugin-modelled-domain-state.md).

## Installation and document behaviour

Installing or removing a plugin changes Kagami's available authoring
vocabulary. It does not mutate an open experiment, an accepted workload, or a
run.

An experiment that uses a plugin retains the stable plugin, model, schema, and
parameter identities needed to diagnose compatibility and compile the same
intent. If the required plugin is absent, Kagami preserves the authored data
but marks the affected model unavailable; it does not substitute another model
or fabricated defaults. Editing or submitting that part of the experiment
requires restoring its exact pinned release/contributions or explicitly migrating
it; a similarly named compatible-looking release is not an automatic replacement.

Releases coexist immutably. Updating changes the default for new authoring but
does not rewrite existing pins or delete old releases. Persistent enablement is
per logical plugin and suppresses every release when disabled; startup overrides
affect only that process. Default selection, enablement, experiment selection and
removal are separate operations. Removal warnings cover known open documents,
not an assumed inventory of every experiment file on disk.

Plugin source locations, installation directories, registries, and archive
encodings are distribution details rather than scientific or workload
identity. Workload submission follows the content-addressed closure rules, so
workers reuse an already cached component by digest and fetch only missing
artifacts.

Built-in gravity and electrodynamics plugins use the same public manifest,
schema, validation, compilation, and workload-component contract as installed
third-party plugins. A hidden privileged model API would make the extension
contract impossible to verify.

## Relationship to object catalogs

Simulation plugins and object catalogs solve different reuse problems:

```text
object template     = reusable authored component/property data
simulation plugin   = contributions: vocabulary and/or computational models
```

An object-template name never selects executable physics. A researcher first
selects simulation models explicitly, then creates or instantiates objects
whose components satisfy those models' schemas. Instantiating a catalog
template copies self-contained authored object state into the experiment;
selecting a simulation plugin causes workload compilation to pin executable
artifacts.

## Object composition and emitters

A modeled object owns intrinsic pose and velocity. Plugin schemas contribute
components rather than species: dynamics supplies inertial mass and is the one
integrator of object motion; without Dynamics the object remains
kinematic/static during a run. There is no persisted motion-authority flag.
Gravity, electrostatic, and other field
couplings supply typed force contributions. Sources and couplings are separate,
and inertial mass, gravitational mass, and electric charge retain distinct
dimensioned meanings. The workload lifecycle combines contributions in stable
order and integrates each dynamic object once. See
[ADR 0020](./adr/0020-compose-object-behaviour-through-plugin-components.md).

An emitter is an ordinary object with an emitter component and may also carry
dynamics and field couplings. Its authored spawn table may refer to catalog
templates, but the authoring command atomically materializes bounded spawn
blueprints into the experiment. Compilation revalidates and copies them into
the immutable workload closure without a live catalog lookup. Runtime consumes only those
blueprints; it cannot consult Kagami's catalog. See
[ADR 0021](./adr/0021-capture-particle-emitter-recipes-in-workloads.md).

## Security boundary and excluded meanings

Field brush/painting is excluded from MVP and retained only as a demand-driven
post-MVP possibility. If needed, it requires a bounded kernel-owned authoring
state-edit operation through the document authority, never mutation via sampling.

In the admitted design, “plugin” does not mean:

- a native dynamic library loaded into Kagami or `orishu-worker`;
- arbitrary UI code with access to the document, filesystem, credentials, or
  renderer;
- unrestricted WASI code with ambient filesystem, network, clock, process, or
  threading capabilities;
- a template, filename, or mutable tag that silently selects solver behaviour;
  or
- code fetched and executed merely because an experiment contains a URL.

Declarative authoring schemas are untrusted, bounded input. Executable code is
digest-verified and runs only through the workload sandbox. Custom native UI,
editor, renderer, or visualization plugins would require a separate security
and architecture decision; generic schema-driven authoring and field
visualization are the initial extension surface.

See [ADR 0008](./adr/0008-catalog-templates-instantiate-self-contained-objects.md),
[ADR 0009](./adr/0009-execute-workloads-as-sandboxed-portable-programs.md),
[ADR 0010](./adr/0010-content-addressed-workload-closure-and-portable-bundles.md),
the [workload contract](./protocol-workload.md), and
[What is an Orishu workload?](./workloads.md).

Worker artifact administration is a separate [planned operator
surface](user-stories/orishu/artifact-administration.md), not plugin installation:
inspect cached kernels/inputs, pre-position verified bytes, reclaim local space
and deny compromised content. Cache eviction permits later refetch; logical
artifact purge suppresses deleted resource identities; digest denial controls
execution regardless of where bytes are stored. The [follow-up task](tasks/implement-artifact-cache-administration.md)
keeps security-policy authority, propagation, persistence and active-run handling
gated on a dedicated design.
