# Project context

Orishu Kagami provides one product for authoring, executing, and observing
scientific simulations. Orishu and Kagami are roles within that product, not
independent platforms.

The accepted MVP plugin boundary has an implemented dependency-light `orishu-plugin`
declaration/identity crate for Kagami and Orishu, alongside variables, catalog and
workload contracts. Pure provider resolution and a stored-ZIP byte codec are implemented.
Selected-closure compilation/verification and exact context-to-declaration checks
are implemented. The [v3 workload extension](docs/workload-v3.md) retains the shared
kernel graph and pins selection/captured-scientific-input descriptors without v2's
global spatial-step/integrator assumptions. V2 identities are unchanged. Shared
fixed-profile compilation/verification and actual Component admission are implemented;
Headless Kagami export now writes a verified portable closure; explicit worker
scientific enablement exposes bounded authenticated load/receipt/current-run
routes. The shared scientific HTTP client implements bounded submission and
correlated receipt/discovery reads without implicit retry or redirects. Kagami
submission and public run control/observations remain unwired.
Shared configuration/domain and instance/validation codecs now feed real reference
Wasm kernels; these alone are not a worker admission authority.
The shared runtime now has an atomic single-partition fixed-profile state owner,
complete in-memory portable checkpoints and bounded detached committed-field
sampling leases. Its raw execution projection alone is not admitted workload
closure; `orishu_runtime::admit` verifies the complete selected root and scientific
inputs before constructing it. Hot-store/arena reuse and product adapters remain work.
Kagami has initial Unix local inventory IO and CLI management; authoring/runtime
adoption remain partial. Exact component pins now survive catalog-v2 and experiment-v3
files (older logical references are preserved without provider selection). Verified
selected component schemas can feed the document authority, preserving constraints,
defaults and scientific roles. Unix startup now supplies installed component schemas,
including process-only overrides; Add authors declared defaults through the same
authority. Dependency-choice dialogs and window export controls remain unwired.
The app-local preparation path can now retain an exact selected artifact closure
under release leases and initialize field/history candidates through the shared
worker sandbox. Shared declaration projection supplies role/channel metadata;
no reference-kernel layout is assembled by Kagami. The document authority now
accepts captured scientific setup as one validated, undoable edit. Legacy setup
and plugin-based setup are distinct variants, never simultaneous domains. Final
configuration expressions and numerical-history inputs must match the capture;
changes requiring reinitialization cannot commit stale state. Model wire v3 exposes
descriptors, not opaque JSON arrays. Scientific file version 4 now stores exact
declaration evidence and captured state in a bounded stored-ZIP/blob container;
save/open restores it without executable installation or initialization. Legacy
JSON v1–v3 semantics remain unchanged. The Unix window now explicitly reinitializes
captured fields through a single bounded background effect, checking document
incarnation/revision/mode and exact inventory availability before one atomic edit.
Creation/configuration flows, dependency controls and MCP scientific parity remain.
The authority now bounds unique scientific buffers plus canonical metadata weight
across live state, undo/redo and replay receipts; pressure may evict old receipts,
never undo history merely to make a capture fit. Field reinitialization separately
bounds retained pending input/selected bytes/output; cancelled work owns its slot
until actual exit. Other effect and IO-buffer reservations remain work. Reopening
does not imply permission to execute.
Captured-scene workload compilation now emits shared scene composition and source
evidence through execution descriptor v2. Runtime admission checks exact component
membership, dimensions and complete agreement with object/coupling packets; source
expressions are non-executable provenance, never worker-side evaluation. Template
fingerprints are historical evidence, not catalog locations or fetch edges. See
[the versioned scene contract](docs/workload-v3.md#scene-bearing-execution-v2).
The Unix `kagami export` command now freezes exact installed code, compiles a saved
captured experiment and publishes a new [portable workload file](docs/workload-bundle-v1.md).
Independent bundle verification/runtime admission needs no plugin inventory.
Emitter/dynamic-membership integration, window export controls and public run control
remain open; scene data does not create a second editable document authority.
The worker's existing formation owner can now issue one generation-fenced
execution reservation for a locked single-node formation. It excludes competing
admission/topology intents and revokes on lifecycle exit; off-owner work must
actually release it before another admission. The internal admission adapter now
verifies portable closures and admits real Components off-owner, links guest
cancellation to that lease, and returns an owner-confirmed handoff. The caller
must retain the lease through runtime disposal. After closure verification the
formation owner now assigns a non-reused workload epoch and an immutable
[run descriptor](docs/run-descriptor-v1.md); callers cannot choose or rebind that
scope. Confirmation and publication recheck the exact owner-issued allocation.
Its retained executor now publishes boundary zero, explicit fixed steps and
terminal stop through the formation owner, then serves bounded immutable field
leases from that accepted state. No guest work or scientific buffers enter the
formation mailbox. Publication coordination loss terminates the lifetime rather
than continuing with uncertain state. The internal projection/allocation does not
advertise distributed execution capability. The separately enabled
[scientific-load HTTP profile](docs/protocol-scientific-load-v1.md) uses daemon-owned
admission and durable receipts, not request-owned execution. Default startup remains
formation-only. Owner-published scientific occupancy uses summary v2 rather than
misreporting the workload as absent; historical acceptance is not live-run availability.
See [ADR 0028](docs/adr/0028-fence-worker-scientific-admission-through-formation-owner.md).
These form one coupled product seam; keep dependency ownership
acyclic and IO in adapters. The crate layout may be reconsidered after MVP.
See [ADR 0027](docs/adr/0027-plugin-contributions-and-immutable-releases.md)
and [X-PLUGIN's design gates](docs/tasks/define-and-implement-plugin-contract.md).

## Product language

Plugins may export dimensioned constants through declarative contributions.
Experiment imports capture exact provider identity and values; installation
updates never silently change existing expression results. Kernels validate
numerical timestep admissibility against their configuration/discretization;
recommendations do not rewrite authored time controls. Integrator history covers
entity births/deaths and is checkpointed with membership. Successful samples
retain evaluation/interpolation/reconstruction quality separately from precision.
Field brush editing is post-MVP, to revisit only if needed.

Physical boundary policies belong to each selected field model's declared
configuration, over the shared geometric domain. Captured resolved quantities keep
their SI dimensions; default expressions are resolved during authoring, not silently
re-evaluated when loading a workload. Unsupported boundary/discretization requests
are rejected, not replaced by a kernel's convenient default.

The initial integrator profile consumes current state, accumulated forces,
declared bounded history and a fixed timestep after one field/force stage.
Integrator-required history is scientific/checkpoint state; observer trajectory
history does not govern the solve. Pre/post-force hooks and repeated evaluations
are deferred until field/Dynamics integration evidence, with explicit field-kernel
compatibility required.

The dynamics integrator is a user-selected plugin kernel, not a runtime-owned
numerical formula. Orishu enforces its declared execution-contract compatibility
and state ownership. MVP plugin distribution uses local bundles only; registries
and discovery are post-launch work, and local kernels remain untrusted.

Each kernel explicitly declares supplied observation channels by scientific
contract reference. A model switch preserves probe requests for missing channels
as unavailable, without deleting them, rebinding by name or fabricating zeroes.

Observable channels name scientific quantities with typed shapes, dimensions,
units and coordinate semantics. Initial shapes are scalars, fixed-size vectors
and fixed-size matrices; Jacobian channels declare their axis/index meanings.
A selected kernel may expose additional channels
such as electromagnetic potentials and field Jacobians; field-family identity
alone does not imply support for every observable.

Sampling-point generation is observer-owned. Kagami is the current observer
application; the public runtime contract remains open to future observers and
independent concurrent subscriptions. Probe surface/volume geometry affects
observer-generated positions, not the kernel sampling interface.

The initial field observation contract is batched point sampling. Probe geometry
(point, finite plane, sphere, box or cylinder) and user-defined sampling density
determine sample positions, not new kernel interfaces. Shape layout semantics
remain explicit instrument metadata, separate from kernel-owned field storage.

Field numerical state is kernel-defined and opaque; the host owns its buffers,
lifetimes and reliable transfer. A kernel exposes typed bounded sampling for
probes, visualization and MCP. Sampling does not mutate scientific state or block
commit. Opaque contents do not remove versioned identity, partition, bounds or
access checks, and host-owned storage does not imply a zero-copy Wasm ABI.

In the initial independent-field execution flow, a field-coupling component
selects an entity's participation in that field kernel. The kernel reads entity
state and produces field state plus force contributions; Dynamics alone reduces
those contributions and integrates motion. Coupling/projection is part of the
field kernel in this profile, not a required separate executable.
The initial pipeline is fixed: compute all field/force contributions from one
committed entity boundary, then reduce and integrate Dynamics once, followed by
validation and atomic commit. Configurable pipelines are future work, not an
initial authoring capability.
Field initialization constructs the kernel's natural default state, which need
not be zero-filled, independently of scene entities. It may perform bounded
kernel setup. Completed-experiment validation is separate; field/object creation
order must not change initial conditions given the same final authored values.
Natural initialization uses kernel-chosen actual output sizes within explicit host
byte/value ceilings, not host-owned field layout formulas. Sufficient different
ceilings must not change scientific output. Completed buffers exclude unused capacity.
Kagami requests field defaults from its local runtime on field creation and
captures the scientific output in the experiment. Reopening and submission
preserve that state; runtime-only setup is reconstructed separately, without
regenerating or replacing authored initial conditions.
Kagami consumes a common runtime interface: local embeds the same execution
engine as a single Orishu worker without requiring formation; a cluster proxy
represents cluster operations without independently stepping state or coordinating
peers. Kernel invocation, buffers and sampling belong behind this interface.
The local runtime remains available for authoring even with a cluster run target.
Manual field reinitialization and domain/compute-parameter edits are normal
authoring operations: the authority captures newly initialized state atomically
with the edit, and undo/redo restore captured states without rerunning kernels.

Provider resolution is authoring-time: reuse exact experiment pins; for unbound
exact-contract dependencies, select the sole eligible provider or ask the user
when ambiguous, then persist the choice. Workloads carry resolved selections;
Orishu validates them without choosing providers or substituting scientific code.
Headless authoring returns a structured ambiguity outcome with eligible provider
choices; the caller submits an explicit selection through the same authority.
It does not wait for an interactive prompt or guess a provider.

| Term | Meaning |
| --- | --- |
| **Orishu** | The distributed execution engine, cluster runtime, shared models, and authenticated client protocol. |
| **Kagami** | The native Orishu client for authoring experiments and inspecting live or recorded observations. |
| **Cluster operator** | A person or agent that provisions workers, forms and secures clusters, and operates them through `orishuctl` or `orishu-monitor`. |
| **Researcher** | A person or agent that uses Kagami to author experiments, submit workloads, and inspect runs without operating cluster membership. |
| **Simulation plugin developer** | An advanced user who implements and tests physical models or numerical methods with external development tools and packages them for Kagami and Orishu. |
| **Cluster** | A running set of nodes cooperating to execute one workload and manage its artifacts. |
| **Cluster formation** | One ephemeral instantiation of a cluster, identified independently of its reusable human-readable name. |
| **Node** | One running `orishu-worker` process participating in a cluster. |
| **Experiment** | Editable intent: a world, physical models, parameters, initial conditions, run controls, and requested observations. |
| **Document authority** | The owner that serializes proposed authoring commands and atomically accepts or rejects experiment revisions. It initially runs inside each Kagami process and may later be hosted by a headless collaboration service. |
| **Variable** | A named, dimensioned value in expression-capable experiment or workload intent, defined by an authored expression and available through an explicit namespace or scope. |
| **Expression** | Retained authored source for a schema-declared numeric value; it may contain literals, units, arithmetic, variables, supported constants, and references to other expression-capable fields. |
| **Resource envelope** | The structural `apiVersion`/`kind`/`metadata`/`spec`/optional-`status` shape shared by Orishu resources and Kagami object templates. It carries no authority, lifecycle, identity, or metadata schema; each resource domain keeps its own. |
| **Object catalog** | Kagami's client-owned, editable collection of versioned object-template files. It is authoring vocabulary, not experiment or workload state. |
| **Object template** | A named, reusable composition of components, parameters, and authored properties that can be instantiated as a self-contained experiment object. |
| **Simulation plugin** | An installable, versioned bundle of contributions: scientific vocabulary, computational models, or both. Vocabulary-only bundles need no executable artifact; computational contributions use digest-pinned sandboxed workload code. The initial contract permits presentation annotations consumed by host-owned generic UI, but no contributed views, windows, renderers, widgets, or executable UI. It is not a native host plugin. |
| **Plugin extension point** | A stable, versioned slot in the shared Orishu–Kagami plugin contract that names one kind of contribution, such as entity-component schemas, field-family declarations, computational models, or observation channels. Orishu Kagami owns extension-point identities and semantics; an external plugin names them in metadata and need not link host code. Contributions use one common envelope, while each recognized extension point owns an independently versioned, strongly typed payload and validator. An unknown point leaves that contribution and its declared dependents unavailable without suppressing understood independent contributions from the same valid release. |
| **Plugin contribution** | One declaration a particular plugin release registers at an extension point. Its canonical identity is provider-qualified: the plugin release, extension point, and plugin-local contribution identity together select it. The extension point decides whether contributions accumulate or require an explicit selection. Display and scientific names are non-authoritative and may collide, so Kagami disambiguates providers and an experiment and workload retain the selected provider and exact contract rather than relying on installation order. A contribution may depend on exact scientific contract identities supplied independently by other plugins; installing it before those dependencies exist is valid, but does not make that contribution available. |
| **Scientific contract identity** | The stable name, version, and canonical digest of a scientific interface contributed through a plugin extension point. It identifies exact semantics independently of which plugin release supplies them. Models declare the exact scientific contract identities they implement; matching names or structural shapes alone never establish compatibility. |
| **Plugin release** | One immutable, content-addressed version of a simulation plugin. Its release identity is derived by tooling from the canonical typed manifest, whose descriptors transitively commit to every registered artifact; authors do not write the identity into the bytes it hashes. Kagami may retain several releases of one logical plugin side-by-side. Updating installs another release and may make it the default for new authoring; it never changes or removes an existing release, and existing experiments remain pinned to their exact release until explicitly migrated. |
| **Plugin bundle** | The isolated, manifest-driven distribution of one plugin release. It may contain schemas, workload components, documentation, examples, and assets, but files register nothing merely by occupying a path: the manifest explicitly declares every contribution. Kagami validates paths, bounds, schemas, and digests before atomically installing content-addressed artifacts. Archive layout and compression are distribution details and do not determine the plugin release identity; releases never overlay a shared union filesystem. |
| **Plugin enablement** | A persistent Kagami user preference on a logical plugin, with an optional process-local command-line override. An enabled plugin's default release contributes choices for new authoring, while an existing experiment may resolve any exact installed release it pins. Disabling suppresses every release without uninstalling it or editing experiments; experiment selection is a separate authored decision. |
| **Contribution availability** | The derived state of one installed plugin contribution against plugin enablement, supported extension points, and its bounded transitive scientific-contract dependencies. A structurally valid plugin release may contain both available and dormant contributions. Missing, incompatible, conflicting, disabled, or unsupported contributions are diagnosed independently and become available when their dependencies do, without reinstalling the release or editing an experiment. A malformed manifest, corrupt artifact, or false release identity instead rejects the whole release. |
| **Kernel** | An independently compiled, content-addressed scientific executable implementing one Orishu-owned execution contract, delivered as an untrusted WebAssembly Component. |
| **Execution contract** | An Orishu-owned versioned interface for a scientific execution role, such as field update or dynamics integration, implemented by a kernel; distinct from the plugin-owned scientific field contract. |
| **World** | The objects and physical properties authored as part of an experiment. |
| **Workload** | The immutable logical bundle required to execute one simulation: a root manifest and the complete closure of digest-addressed compute code, initial conditions, and other required inputs. It is independent of distribution format. A cluster runs at most one workload at a time. |
| **Workload manifest** | The immutable root definition of a workload, including its compute definition, execution requirements, and digest-pinned references to required artifacts. |
| **WebAssembly Component** | The binary technology used to package and sandbox a kernel; not a second scientific entity or a plugin bundle. |
| **Kernel instance** | One configured use of a kernel in an immutable workload graph, with stable instance/model/schema identity, declared state ownership, phases, channels and limits. |
| **Workload component / component instance** | Legacy design and current code/wire names for kernel / kernel instance; renaming prose does not migrate serialized types or versions. |
| **Workload component graph** | The identity-bearing component instances, typed channels, deterministic step plan and scientific placement constraints compiled into a workload. Orishu chooses a legal runtime placement. |
| **Step plan** | The bounded, versioned dependency schedule by which Orishu invokes component-instance phases to propose one candidate simulation boundary. |
| **Workload closure** | The root manifest plus every content-addressed component, initial condition, geometry, and other artifact reachable from it and required to execute the workload. |
| **Selected contribution closure** | The bounded transitive closure of the exact plugin contributions selected or referenced by one experiment, their scientific contracts, and their required artifacts. Kagami compiles this closure into the workload; it never copies whole plugin bundles or unrelated executable contributions merely because they share a release. |
| **Artifact descriptor** | An immutable, identity-only reference to one artifact: its role, algorithm-tagged content digest, exact byte size, media type, and schema compatibility. It contains no retrieval location, so changing where bytes come from never changes the workload. |
| **Canonical encoding** | The one deterministic byte form of a workload manifest, over which its digest is taken. JSON and YAML are authoring inputs that parse into the typed model; they never define identity, so whitespace, key order, comments, and codec choice cannot change a workload. |
| **Workload digest** | The digest of a manifest's canonical encoding, and therefore the identity of the logical workload. Because every dependency is named by digest, it commits to the whole closure. Distinct from the cluster-assigned resource identifier and from the workload epoch, neither of which can affect it. |
| **Workload bundle** | A portable import/export representation carrying a workload manifest and its complete artifact closure; it is transport, not workload identity. |
| **Distribution format** | A physical file layout, archive encoding, or transfer protocol that carries a workload manifest and artifact bytes without changing their logical identities. Multiple formats may carry the same workload. |
| **Run** | One execution of a simulation problem: state advanced through accepted fixed steps, published as an ordered stream of computed states. A run is a local Kagami preview or a workload executing on a cluster. |
| **Run descriptor** | An immutable versioned fact record binding the protocol formation ID, exact workload root and workload epoch. Its canonical CBOR digest is the runtime/sampling run source. Worker allocation belongs to the formation owner; labels, routes, credentials and current state are excluded. |
| **Run reference** | Shareable identification of one cluster run by cluster formation, workload identity, and workload epoch, plus optional non-authoritative connection hints. It contains no credentials or presentation state. |
| **Simulation boundary** | A committed simulation time at which workload state is consistent and safe to observe, checkpoint, or transfer. |
| **Observation** | Versioned output carrying workload, simulation-time, model, numerical, and execution provenance. |
| **Observation baseline** | A complete observation that a consumer has validated and adopted and against which a later observation delta is explicitly encoded. |
| **Result artifact** | Durable workload output intended for analysis, export, or visualization. It is not resumable. |
| **Checkpoint artifact** | Durable state captured at a simulation boundary and suitable for resuming a workload. |
| **Artifact record** | Immutable authority for a checkpoint or result artifact's identity, chunks, integrity, completeness at creation, and committed provenance; it contains no live location. |
| **Result sequence** | A derived operator/client grouping of immutable result artifacts from one run lineage across stop/resume; it is not a stored resource. |
| **Local inventory** | A node-local, potentially stale claim about artifact chunks a worker can serve; it is advisory until bytes verify. |
| **Availability view** | A transient projection of current artifact retrievability derived from records, membership, inventory, verified reads, and backend state. |
| **Membership tombstone** | A formation-scoped barrier recording removal of a node and preventing readmission until explicitly cleared. |
| **Purge tombstone** | Persisted artifact-deletion intent that prevents stale inventory from resurrecting a deleted artifact; its cross-formation retention remains an open storage contract. |
| **Durability target** | The accepted number and placement policy for verified artifact copies; current retrievability does not prove it is satisfied. |
| **Subscription** | The observations, channels, spatial region, and level of detail Kagami currently requests. It never changes simulated values. |
| **Presentation state** | Camera, selection, window layout, visibility, and display density owned locally by Kagami. |
| **Modeled object** | An experiment entity with intrinsic pose and velocity whose physical behaviour is composed from plugin-qualified components. |
| **Field family** | A physical quantity defined conceptually at every point of an authored domain, such as electromagnetic, gravitational, or hydrodynamic-medium state. |
| **Computational model** | One plugin-contributed, executable method selected to evolve a field family. Alternative models for one family are mutually exclusive within a workload unless a future model explicitly defines their composition. |
| **Observation instrument** | An authored, non-perturbing measurement or sampling request such as a probe or region; it cannot contribute to a solve. |
| **Particle emitter** | A modeled object whose emitter component creates bounded run-state objects from self-contained spawn blueprints materialized into the experiment and copied into the workload. |
| **Default view** | Optional client-owned presentation settings saved beside experiment intent. It has its own revision and participates in file dirty/save state, but remains excluded from experiment revisions, undo, workload identity, and runs. |
| **Scene scale** | Client-owned metres per viewport unit, defining render conversion and scale-relative camera reach. Saved with the default view, never a rescaling of authored SI values, the domain, or solver resolution. |
| **Workspace mode** | Kagami's explicit Authoring or Observation/replay UI state; it controls which authority and controls the window exposes. |

## Ownership

- Kagami owns editable experiment intent and presentation state.
- Every authored change, whether initiated by the UI, an MCP client, undo, or
  redo, is a proposed command decided by Kagami's document authority. No input
  adapter mutates the experiment directly.
- Kagami's document authority owns variable definitions, expressions, their
  dependency graph, and their resolved values as part of experiment intent.
- Kagami's separate catalog authority owns object-template files, validation,
  and catalog revision. UI and MCP adapters command that same authority.
- A template identity has exactly one owner, whatever an entry's load state,
  and every catalog file effect stays physically inside the configured catalog
  root. Both are decided before anything is written, so a refused command
  leaves the files, the projection, and the revision untouched.
- Kagami owns the installed simulation-plugin inventory and exposes its
  declarative model vocabulary to authoring. Plugin installation never grants
  executable code ambient authority inside Kagami.
- Submission compiles supported experiment intent into an immutable workload
  manifest, component-instance graph, step plan, and input artifacts.
- Orishu owns the accepted workload, workload epoch, simulation boundaries,
  partition ownership, checkpoints, result artifacts, and execution provenance.
- Orishu authoritatively validates and freezes the variable graph and resolved
  values of an accepted workload; Kagami's evaluation is authoritative only
  for its editable experiment revision.
- Kagami uses Orishu's client protocol. It never joins cluster membership or
  participates in peer coordination.
- Local preview and remote execution must publish the same observation
  semantics; neither renderer nor UI may read solver-owned memory.
- Draft sharing is initially file-based. A future headless collaboration
  service must host the same document authority and command semantics rather
  than introduce a second experiment model.
- The Orishu client interface presents one authoritative service, but its run
  authority is the state collectively committed for a workload identity,
  workload epoch, and simulation boundary—not the incidental node serving a
  client connection.

## Invariants

- Simulation time advances only through accepted fixed steps.
- A rendered value identifies the workload, simulation boundary, model,
  precision, domain, and validity that produced it.
- Editable scene intent never mutates an already loaded workload.
- Identified workload-load receipts record historical admission, not mutable run
  status. Durable pending intent precedes execution permission; an interrupted
  unresolved attempt is indeterminate, never implicit permission to execute again.
  The daemon retains admission and accepted runs independently of request/response
  lifetimes. Losing a response cannot unload or replay execution; daemon shutdown
  or owner drop revokes its active work and run loans.
  This worker-local history does not provide distributed exactly-once execution
  or restore scientific state. See [ADR 0030](docs/adr/0030-retain-durable-worker-load-receipts.md).
- Run observations never mutate editable experiment intent. Adopting computed
  state into an experiment is an explicit, validated authoring command that
  creates a new revision and retains source provenance.
- Expression-capable values retain their authored source and are adopted only
  when their complete affected dependency closure resolves to finite values of
  the dimensions required by the experiment schema.
- Instantiating a catalog template persists complete authored object state and
  source provenance in the experiment. Catalog changes never silently mutate
  existing objects. A separately authored catalog-qualified variable reference
  remains an explicit authoring dependency; workload compilation captures and
  rewrites its complete closure so Orishu never depends on a Kagami catalog.
- A modeled object owns intrinsic pose and velocity and composes physical
  behaviour from plugin-qualified components. Dynamics owns inertial
  integration; without Dynamics it remains kinematic/static during a run.
  Field couplings contribute forces to Dynamics. There is no separate persisted
  motion-authority flag. No template, species,
  filename, or component display name selects hidden executable physics.
- An experiment authors a bounded domain and selects field families and exactly
  one computational model for each selected family. Models such as Coulomb and
  Maxwell/Yee may expose the same stable electric-charge coupling while being
  mutually exclusive implementations; classical gravity and GEM follow the
  same rule for gravitational coupling. A plugin supplies both declarative
  schemas and the digest-pinned executable update code.
- An observation instrument is authored intent but is not a physical object:
  it requests or visualizes values and cannot contribute to a solve. A detector
  intended to perturb the experiment is modeled as an object with components.
- A particle emitter is a modeled object with an emitter component. Workload
  authoring atomically captures catalog-selected spawn definitions as bounded,
  self-contained experiment blueprints. Compilation revalidates and copies
  them into the workload; spawned objects are deterministic run state, not
  experiment revisions, and Orishu never reads the Kagami catalog.
- Accepted workload expressions are immutable for their workload epoch and are
  never reevaluated as runtime control state. Changing one requires a new
  workload resource and normal replacement.
- Workload components execute as untrusted guests. They have no ambient host
  authority, interact only through their declared lifecycle imports/exports,
  and cannot commit partial output after a trap, timeout, or limit violation.
- Orishu hosts and orchestrates the admitted workload component graph. Guests
  exchange state only through bounded typed host-mediated channels; the
  deterministic step plan, not installation order or completion timing, decides
  dependencies and reductions. Any required failure rejects the whole candidate
  boundary.
- Field-model components own their field state, coupling/projection phases
  produce typed entity contributions, and Dynamics alone integrates particle
  velocity and position. Orishu may co-locate or distribute eligible component
  partitions without changing workload identity or scientific ordering.
- Workload identity contains no artifact location. Changing a peer, mirror,
  cache, repository, archive encoding, or other distribution detail never
  creates a different workload when the root and artifact digests are unchanged.
- Presentation state never changes physical execution.
- A saved Kagami file may carry a versioned client-owned default view beside
  experiment intent. Authoring camera/projection edits advance the view revision
  and dirty the file. The view is excluded from the experiment revision, undo
  history, workload identity, and run; playback view changes are ephemeral and
  opening a file never resumes a saved run.
- Multiple clients observing one run own independent subscriptions, cameras,
  selections, playback cursors, and playback rates. Sharing a document or run
  reference does not implicitly synchronize presentation state.
- Observation projections may be coalesced or skipped under backpressure, but
  every delta names a compatible complete baseline and an unusable baseline
  recovers through a full snapshot. Halo data, step decisions, commands,
  checkpoints, and workload artifacts are never placed on that lossy path.
- A committed observation logically describes the whole requested experiment
  state, including complete selected-field state. Region, channel, and level-of-
  detail subscriptions are delivery projections. Their bounded resource use is
  isolated so an observer cannot change scientific state or block run commit.
- Interpolated or extrapolated observations are presentation-only and cannot
  become results, checkpoints, simulation inputs, or authored state without an
  explicit authority transition.
- Network input is untrusted, bounded, and validated before allocation or state
  adoption.
- The runtime is decentralized and runs one workload per cluster; it is not a
  general scheduler or job queue.
- Cluster and node names are labels, not identity. Formation and membership
  identities follow ADR 0013.
- Sharing the resource envelope confers no authority. A synthetic projection
  does not become durable operator-authored configuration by carrying the same
  five fields, and metadata schemas, identities, and validation stay owned by
  each resource domain rather than generalised into the shared shape.
- Artifact records, local inventory, availability views, and purge tombstones
  retain their separate authority levels as defined by the storage spec.
- Worker operational metrics, traces and process probes are bounded diagnostic
  projections owned by the IO shell (ADR 0017). They are not scientific
  observations, membership decisions, durable audit or committed provenance;
  unavailable telemetry cannot block or change authoritative transitions.
- Shared numerical kernels do not depend on Kagami UI code or Orishu peer
  runtime code.
- Built-in and third-party simulation plugins use the same public authoring,
  validation, workload-compilation, and sandbox contracts.
- Selecting a simulation plugin pins its model/schema identities and executable
  artifacts into the compiled workload closure. A plugin name, installation
  path, source URL, or mutable version tag never selects executable physics at
  runtime.
- Removing or updating an installed plugin never mutates an experiment,
  accepted workload, or run. Missing capabilities are explicit and are never
  replaced with a plausible default model.
