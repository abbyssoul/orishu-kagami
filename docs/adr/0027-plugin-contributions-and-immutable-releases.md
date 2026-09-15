# ADR 0027 — Plugins bundle contributions in immutable releases

Status: Accepted architecture and policies; the [concrete v1 contract](../plugin-contract-v1-draft.md) was also accepted on 2026-09-16. Implementation/conformance evidence remains tracked in [X-PLUGIN](../tasks/define-and-implement-plugin-contract.md).

## Context

Packaging and extensibility are different concerns. Researchers need familiar
install/update/enable/disable workflows, while independently developed plugins
must compose scientific vocabulary and computation. A vocabulary plugin can
declare gravity and coupling mass; other plugins can supply classical or GEM
models of that field. Neither must be developed or distributed together.

The exported workload must execute without the author's Kagami installation.
Installing a bundle must not implicitly select its physics or ship every kernel
in that bundle to a worker. Existing catalog/document plugin identifiers and
schema registries are foundations, not an implementation of this contract.

This refines [ADR 0020](0020-compose-object-behaviour-through-plugin-components.md),
[ADR 0023](0023-fields-are-plugin-modelled-domain-state.md), and
[ADR 0024](0024-orishu-orchestrates-a-workload-component-graph.md).

## Decision

### Extension points, contributions and scientific contracts

- Orishu Kagami owns stable, versioned extension points. External plugins name
  them in metadata; linking a Rust SDK is not required. A common contribution
  envelope carries a strongly typed, independently versioned payload for each
  recognized extension point. The initial exact point names/payloads still need
  specification; examples in design prose are not frozen identifiers.
- A plugin is a manifest-driven bundle of contributions. Vocabulary-only,
  computational-model-only and combined bundles are valid. Components are
  additive; alternative computational models are offered as choices, with one
  governing model per active field family, not an installation conflict.
- A contribution is identified by immutable plugin release, extension point
  and plugin-local contribution ID. Display/scientific names may collide;
  provider choices must not depend on installation order.
- During authoring, reuse an experiment's existing exact provider pin. For an
  unbound dependency, resolve automatically only when exactly one eligible
  provider exists; Kagami asks the user when several are eligible. Persist the
  resulting exact selection. A missing/unavailable pin requires resolution or
  explicit migration, not automatic substitution. This selects a provider of an
  already required exact contract, not a scientific model on the user's behalf.
- Headless authoring exposes ambiguity as a structured protocol outcome with
  eligible provider choices. The caller supplies an explicit selection and
  retries through the same authority; there is no prompt, indefinite wait or
  guessed default. Interactive Kagami presents that same outcome as a choice.
  Options and diagnostics must be bounded, with precise wire representation
  specified in X-PLUGIN. A subsequent command is validated against current
  availability; an earlier offered option is not authorization to bypass checks.
- A scientific contract has a stable name, version and canonical scientific
  digest, independent of its provider release. Models depend on exact contracts,
  not necessarily a particular vocabulary plugin. Name or structural shape
  alone cannot establish compatibility. Presentation metadata is excluded from
  this scientific digest; the exact canonical projection remains to be defined.
- Availability is contribution-level. Missing, incompatible, disabled or
  unsupported dependencies leave that contribution and its transitive dependents
  dormant; independent contributions of the same valid release remain usable.
  Installing model B before vocabulary A is valid. Resolution is bounded and
  diagnoses dependency cycles. No activation-group abstraction is needed initially.
- Unknown extension-point payloads remain verified opaque artifacts and dormant
  contributions. They do not reject known independent contributions. Malformed
  common manifests, corrupt artifacts and false release digests reject the whole
  release atomically; declarations cannot hide coupling to an unknown payload.

### Packaging, identity and lifecycle

- MVP distribution is local-bundle-only. Registry, discovery and remote update
  services are post-launch; publisher signatures are not an initial requirement.
  Explicit local updates preserve side-by-side immutable releases. Local origin
  is not trust: all validation, bounds and sandbox requirements still apply.

- Bundles are isolated and manifest-driven, not a union filesystem. Paths alone
  register nothing. Artifacts are content-addressed internally; archive layout,
  compression and source location do not determine logical identity. Installation
  validates paths, bounds, decompression and digests before atomic admission.
- `PluginReleaseId` is tooling-derived from a canonical typed root manifest
  committing to every declared package artifact descriptor. It is not the raw
  archive hash or an author-chosen version string. Changing a declared document,
  icon or code artifact changes the release; repacking unchanged logical content
  does not. The release ID is excluded from its own hash and full contribution
  identities are derived afterward.
- Authors use ergonomic manifests and local paths, compile code with external
  tools, then use Kagami validation/packaging tools to hash artifacts, construct
  descriptors, canonicalize the root and report the release identity. These tools
  inspect sandboxed components without executing them. Installation independently
  verifies the result. Kagami is not a source editor or compiler.
- Immutable releases coexist. Updating installs a new release and changes the
  default for new authoring without modifying or deleting old releases. Existing
  experiments pin exact releases until an explicit migration. Removal is explicit;
  warnings about known open documents must not claim discovery of every file on
  disk. Automatic garbage collection based on such discovery is not accepted.
- Enablement is a persistent preference for a logical plugin, with process-only
  startup overrides. Its default release supplies new-authoring choices; pinned
  installed releases can still serve existing experiments when enabled. Disable
  suppresses all its releases without uninstalling or editing documents. Built-ins
  follow the same rules; Kagami remains usable with all simulation plugins disabled.
- Headless commands use the same `kagami` executable and inventory authority as
  UI/MCP adapters. The planned command surface covers list, inspect, validate,
  pack, install, update, set-default, enable, disable and remove, plus startup
  overrides and expected-release verification. Exact syntax is a draft in
  [the plugin design](../simulation-plugins.md#planned-kagami-plugin-commands).

### Workload closure and shared ownership

- Accept the narrow first-pass integrator profile: current state, accumulated
  forces, declared bounded history and `dt` produce candidate state/history after
  one field/force stage. Defer pre-/post-force hooks and multi-evaluation stepping
  until fields and Dynamics work together, then review with evidence. Repeated
  force evaluation requires explicit support from field kernels as well as the
  integrator. Numerical history is checkpointed scientific state, distinct from
  independently retained observer trajectories; a circular buffer is a possible
  implementation, not a mandatory wire layout.

- The researcher selects a plugin-contributed dynamics-integrator kernel; its
  formula belongs to the plugin, not Orishu. Euler/Verlet are Field CAD examples;
  other methods may be contributed. The host owns scheduling/contract validation,
  not the numerical algorithm. Required force evaluation stages and carried
  history must be declared; a single accumulated-force input does not imply
  support for arbitrary multi-stage integrators. Initial profile compatibility
  remains a specification gate, not permission to silently alter a method.

- Observable channels have stable scientific identity, typed shape, dimensions/
  canonical SI units and coordinate semantics. Kernel-dependent electromagnetic
  potentials and field Jacobians are explicit design cases, including Jacobians
  for flow-line consumers. They use ordinary point sampling. Initial value shapes
  are scalars, fixed-size vectors and fixed-size matrices, with explicit
  axis/index semantics, frames and dimensions for Jacobians. Concrete encoding,
  size limits, precision and declaration encoding remain to be specified.
  Each kernel explicitly declares supplied channels by scientific contract
  reference. An otherwise valid model switch preserves unsupported probe-channel
  requests as unavailable with structured reasons, never deleted or replaced by
  zeroes. This refines older model-switch wording that blocked on any unsupported
  observation request; scientific initial-state incompatibilities remain separate.

- Required initial observation is batched point sampling. Point, finite-plane,
  sphere, box and cylinder probes with user-defined sampling density/count reduce
  to collections of positions; the kernel need not implement shape-specific
  sensor contracts. Point generation is observer-owned; Kagami is the current
  observer application, not a restriction on future observers or concurrent
  clients. Surface/volume modes are instrument features; exact geometry layout,
  density and generation rules belong to K8/K-OBSERVATION, not X-PLUGIN gates.
  Kernel-side sampling bounds remain mandatory. Richer measurement operations
  are deferred; attachment, batching and routing preserve snapshot identity and
  sample correspondence.

- Kagami consumes a common runtime interface with local and cluster-proxy
  implementations. Local embeds the same execution engine used by a single
  Orishu worker without requiring formation. The proxy represents the cluster
  through client operations; it does not advance state or coordinate peers.
  Kernel invocation, buffers and sampling live behind the runtime boundary.
  Local initialization/reinitialization remains available for authoring even
  when execution targets the cluster, using installed pinned kernels without a
  live cluster connection. Returned state is captured by the document authority;
  runtime execution is not a second authoring authority.

- Field state may be opaque kernel-defined storage. The mandatory public
  observation boundary is typed bounded sampling, not a host-defined numerical
  layout. Scientific channels declare shape, dimensions and units; samples
  identify their state/boundary and validity. Sampling remains isolated from
  scientific mutation and step commit. A kernel supplies values, not visual UI.
- The host owns buffer lifetime/allocation and reliable opaque transfer between
  compatible instances of the pinned kernel. State/exchange metadata still binds
  bytes to workload, field instance, schema, boundary, partition and coverage.
  Reuse and avoiding redundant copies are goals; zero-copy Wasm access is not
  assumed. Invocation-scoped grants, immutable committed inputs and isolated
  candidate outputs remain mandatory. A bounded-copy implementation is valid.
- Opaque state replay with new queries requires the pinned sampling code and
  required artifacts. Sample-only recordings promise only their recorded
  measurements. Reject host-defined dense storage as a universal requirement,
  and reject opaque observation outputs that generic consumers cannot interpret.

- Reinitialization is a normal authoring operation: the scene inspector offers
  it explicitly, and field-domain/compute-parameter edits perform it as part of
  the edit. The authority atomically captures settings and affected field states
  in one undoable revision; failure preserves the prior revision. Undo/redo use
  captured states, not new kernel executions. This replaces the tentative policy
  of rejecting such edits and requiring a separate reset command. The reset
  effect is visible, UI/MCP share the authority, and run/replay state is untouched.

- Field `init` constructs its kernel-defined natural default state and performs
  bounded sandboxed setup. It does not receive coupled entities or compute a
  source-consistent initial field. The natural default is not necessarily all
  zeroes. The completed authored field/entity collection defines initial conditions;
  creating fields before versus after objects must not change those conditions
  given identical final authored values. Separate completed-experiment validation
  reports scientific incompatibilities without silently rewriting the setup.
  First-step field updates receive entities. Kagami runs default construction in
  the local sandbox on field creation and captures its scientific output in the
  experiment. Reopening/submission preserve and load/export that captured state,
  never regenerate defaults implicitly. Runtime-only setup is reconstructed
  separately without altering scientific state. Exact exports and storage formats
  remain to be specified; setup cannot introduce untracked scientific state that
  bypasses deterministic execution or checkpoint/restore requirements.

- The first-pass scientific pipeline is fixed: field kernels compute candidate
  fields and force contributions from their own prior field state and the same
  read-only committed entity boundary; Dynamics then reduces all contributions
  in deterministic order and integrates each dynamic entity once. Validation and
  atomic commit follow. No field observes partially integrated entities. This
  narrows ADR 0024's general graph to an initial supported profile, without changing
  host orchestration or state ownership. Configurable pipelines remain future
  extensions, not an initial implementation requirement. The pipeline does not
  by itself choose the integrator formula or field temporal discretization.

- A **kernel** is one independently compiled scientific executable implementing
  one Orishu-owned **execution contract**, such as field update or dynamics
  integration. A **kernel instance** is its configured graph use. **WebAssembly
  Component** denotes its binary technology, not another scientific layer.
  Existing code/wire names `workload component` and `component instance` remain
  legacy spellings pending explicit migration. This supersedes ADR 0024's earlier
  use of “numerical kernel” solely for an internal algorithm/library.
- Execution contracts are platform-owned, unlike plugin-owned scientific
  contracts such as gravity vocabulary. A plugin can declare gravity and supply
  classical/GEM kernels, each independently compiled against the field-update
  execution contract. Plugins may bundle multiple kernels and kernels may share
  source libraries; independently selectable implementations remain separate
  executable artifacts. Common lifecycle methods and several execution phases
  are compatible with one scientific execution contract. This is a product rule,
  not a claim that Wasm cannot contain multiple implementations or that sandboxing
  proves the numerical meaning of arbitrary code.

- Export captures only the selected transitive contribution closure: required
  scientific contracts/schemas, executable artifacts, inputs, resolved parameters,
  graph/step plan and provider provenance. Unused kernels and authoring-only
  documentation/assets are excluded. If mass comes from A and the chosen solver
  from B, A's unused solver must not accompany the workload merely because it was
  installed. This is a least-authority requirement as well as a size concern.
- Workers independently validate the immutable workload and do not require a
  plugin inventory, package registry, catalog or author's filesystem. The
  selected-release provenance proof needs concrete specification. Kernel artifact
  granularity is settled above: export selects independent compiled artifacts,
  never strips functions or relinks a multi-solver artifact inside Kagami.
- Provider choice is finished before workload export. Admission verifies pinned
  selections and their closure; it never prompts, chooses an installed provider,
  or substitutes a compatible-looking model. Missing local artifact bytes may be
  obtained by their exact digest through the workload transfer path. An unresolved
  selection is invalid workload intent, not a request for cluster-side resolution;
  an unavailable or invalid required artifact prevents admission/execution.
- The initial contract allows scientific schemas and bounded presentation
  annotations interpreted by host-owned generic UI. It does not allow contributed
  windows, views, widgets, renderers, executable UI or a layout language.
- Introduce a dependency-light shared `orishu-plugin` crate for pure identities,
  typed contribution contracts, canonical encoding and bounded validation/resolution.
  Archive/filesystem/network/CLI and inventory persistence belong in Kagami's IO
  shell. Workers consume required scientific contracts through workloads rather
  than installing packages. Audit existing type ownership before moving it: the
  dependency graph must remain acyclic and `orishu-workload` must not acquire IO.
- Variables, catalog, workload and plugin contracts form the coupled shared
  Orishu–Kagami seam in this monorepo, not independently evolving platforms joined
  by translation layers. The MVP crate split can be revisited after implementation
  evidence; final per-type placement is not settled by the crate names alone.

## Options considered and rejected

- Path overlays/magic registration: familiar in resource packs, but introduce
  override-order authority instead of explicit contribution selection.
- Logical names, installation-local IDs, or name/version alone as exact identity:
  cannot distinguish independent providers or changed scientific meaning.
- Every model depending on a specific vocabulary package release: unnecessarily
  couples independent development; exact scientific contracts provide the seam.
- Whole-plugin disablement for a missing/unknown contribution: prevents useful
  independent contributions and installing models ahead of their vocabulary.
- In-place updates or automatic rebinding: silently change authored scientific
  intent. Immutable side-by-side releases and explicit migration retain it.
- Including whole plugins in workloads: adds irrelevant code and authority and
  misrepresents which model was selected.
- Executable visual extensions in the initial contract: expands the security and
  UI compatibility boundary before the scientific contract is established.
- Duplicated Kagami/worker contracts or immediate post-MVP crate decoupling:
  undermine the shared seam; the accepted MVP uses one pure shared definition.

## Consequences and remaining decisions

### Accepted refinements from the Field CAD code review

- Add declarative exported dimensioned constants to the shared variables contract.
  Capture imported values/provider provenance in experiments/workloads; never
  adopt the PoC's duplicate-registration replacement behavior.
- Scientific validation explicitly receives timestep, discretization, configuration
  and profile. Every selected field/integrator kernel checks numerical admissibility;
  optional recommendations never silently change authored `dt`.
- Integrator history has an explicit bounded birth/death and cold-start contract,
  tied to committed membership and checkpoint restoration. Missing required history
  is not silently zero. Exact scheduling is reviewed with ADR 0021/X-EMITTER.
- Sampling preserves successful-value quality independently of numeric precision
  and uses flat bounded buffers, validity/quality arrays and explicit leases rather
  than per-cell allocations. Cache identity includes the snapshot and query.
- Field brush/painting is not MVP. Reconsider only on demonstrated demand; any
  eventual state-edit interface is separate from read-only sampling.

The review confirms the PoC Verlet's pre-force half-drift belongs to the staged
profile follow-up. No change to the narrow first-pass pipeline is implied.

[X-PLUGIN v1](../plugin-contract-v1-draft.md) consolidates the concrete resolutions.
The user explicitly accepted it on 2026-09-16, including identifiers, formats,
limits and migration policy. Pure-contract implementation may now proceed;
acceptance does not claim generated ABI bindings or migration fixtures exist.

Exact release/contribution pins require versioned catalog/document/workload
integration, not silent reinterpretation of current IDs. Installation success,
availability, experiment selection and run admission become distinct outcomes.
The implementation task must test these through public interfaces and hostile
serialized input, while preserving existing workload canonical identity fixtures.

The former provider, identity, archive and migration-policy questions are settled
by that explicit acceptance. Remaining engineering evidence includes concrete
machine-readable schemas/golden vectors, WIT bindings and host execution budgets,
and audited document/workload version identifiers with compatibility fixtures.
Registry discovery and publisher trust remain post-launch, while artifact-admin
policy has its own security design gate. [X-PLUGIN](../tasks/define-and-implement-plugin-contract.md)
tracks bounded implementation work; none of these follow-ups reopens the accepted
local MVP design implicitly.
