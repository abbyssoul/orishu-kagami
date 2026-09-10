# User stories: Kagami experiment authoring

User stories in this file are written from the perspective of a
scientist-researcher using Kagami as a CAD-like desktop application for
crafting physics simulation experiments and running them. They are not written
from the perspective of a cluster user or administrator, which are captured in
[docs/user-stories/orishu](../orishu/README.md). Where a Kagami story delegates
to cluster-side behavior, it links to the corresponding orishu workload story
instead of restating it.

## The two entities

Kagami's product model has two entities, and every story belongs to one of
them:

- **Experiment** — the editable intent a user crafts: the world (modeled
  objects, their composed physical components, and non-perturbing observation
  instruments), the computational domain and discretization,
  simulation parameters (time step and related controls), which fields and
  physical models are active, initial conditions, and requested observations.
  An experiment is saved to and loaded from a versioned **experiment
  document** (a file). This is the entity Field CAD called the "scene"; this
  repository uses "experiment", and the viewport content is the experiment's
  world.
- **Run** — one execution of an experiment: an ordered stream of computed
  states at committed simulation boundaries. A run is either **local**
  (in-process preview inside Kagami) or a **cluster run** (the experiment
  submitted to an Orishu cluster, where it becomes an immutable workload). A
  run's durable outputs are result artifacts and checkpoint artifacts, as
  defined in the [orishu glossary](../orishu/README.md#glossary).

The experiment is editable and owned by Kagami; a run's computed states are
immutable output with provenance. Editing an experiment never mutates a run,
and a run never mutates the experiment that produced it.

The app makes that boundary visible as two workspace modes. **Authoring** shows
and edits the initial experiment. **Observation/replay** shows a particular
run and offers playback and visualization without experiment-editing controls.

## Experiment documents

### Create a new experiment

As a scientist-researcher, I want to create a new experiment so that I can
start authoring a reproducible physics simulation.

**Given** the Kagami app is open
**When** I create a new experiment (File > New or its keyboard equivalent)
**Then** a new, valid experiment opens in the editor, ready to be edited and run.

**Acceptance criteria:**
- A new experiment is available from the File menu and a keyboard shortcut.
- I can choose at least between an empty experiment and a demonstration
  experiment that exercises the authoring workflow.
- The new experiment opens in a valid, editable state — computational domain,
  default simulation parameters, and either an empty world or the template
  content — such that it can run without further setup.
- Creating the experiment leaves no opening undo entry: undo is unavailable
  immediately after creation.
- If the currently open experiment has unsaved changes, Kagami offers to save
  them before replacing the experiment; declining keeps the current
  experiment open and nothing is lost.
- The new experiment is clearly marked as unsaved (no file location yet) and
  becomes modified only after the first edit.

### Save an experiment to a file

As a scientist-researcher, I want to save my experiment to a file so that I
can continue, share, and reproduce my work later.

**Given** an experiment is open in the editor
**When** I save it (File > Save or Save As)
**Then** the experiment is written to a file and the saved state is clearly
indicated.

**Acceptance criteria:**
- Save writes to the experiment's current file; Save As lets me choose a new
  location, which then becomes the current file.
- The saved document captures the complete editable intent: the world
  (objects and their physical properties), computational domain and
  discretization, simulation parameters, active physics and field choices,
  initial conditions, and requested observations.
- The document is versioned and carries enough provenance (schema, model, and
  catalog versions; units) for Kagami to detect compatibility when opening it
  later.
- The document persists experiment intent and may also carry a separately
  versioned client-owned default view. Computed states, results, active-run
  state, credentials, and run history are never part of it.
- After a successful save both the experiment revision and authoring-view
  revision are marked saved. Later scientific edits or supported authoring
  camera/projection edits mark the file modified again. The file name and one
  combined modified state are visible in the window.
- If the file cannot be written (for example: missing permission, invalid
  path, or insufficient disk space), the open experiment is left intact and a
  clear error explains what failed.

### Share an experiment draft

As a scientist-researcher, I want to share an experiment document with a
collaborator so that they can inspect or continue the draft without requiring
a live Kagami service.

**Given** a saved experiment document
**When** I copy it through a file-sharing or version-control mechanism
**Then** another compatible Kagami instance can open the same authored intent.

**Acceptance criteria:**
- The experiment file is self-contained with respect to objects materialized
  by catalog instantiation. If its authored expressions deliberately retain
  catalog-qualified variables, the recipient can still open and inspect the
  source but needs copies of the referenced catalog files and compatible
  plugin schemas to validate, edit, or compile those expressions.
- The file does not silently carry a live cluster connection, run-control
  capability, credentials, or observation cache. It may carry a client-owned
  opening view, which never affects experiment meaning or synchronizes cameras.
- Opening a shared copy creates an independent local authoring session. Later
  edits to either copy do not propagate automatically.
- Simultaneous edits are reconciled explicitly by people or external version
  control; the initial product does not claim live multi-writer semantics.
- Sharing and opening preserve stable experiment/object identities and authored
  expression source so provenance and later diffs remain meaningful.

### Open a previously saved experiment

As a scientist-researcher, I want to open a previously saved experiment so
that I can continue or reproduce prior work.

**Given** a saved experiment document
**When** I open it (File > Open)
**Then** the experiment is restored in the editor as it was authored.

**Acceptance criteria:**
- All authored state is restored: world, domain, simulation parameters,
  physics choices, initial conditions, and requested observations, with stable
  object identifiers preserved.
- A saved default projection, camera pose, focus/orbit and other supported
  opening-view settings are restored separately from experiment intent.
- An opened experiment is restored paused and unmodified; opening a file
  never starts a simulation or by itself consumes compute resources.
- An incompatible, unknown, or corrupt document is rejected with a clear
  reason (for example: unsupported document version, or missing referenced
  data); Kagami never silently reinterprets or partially loads it.
- If the currently open experiment has unsaved changes, Kagami offers to save
  them first; declining keeps the current experiment open.
- Opening a document leaves no undo entry for the load itself: undo does not
  revert to whatever was open before.

## Editing the experiment

### Modify the experiment

As a scientist-researcher, I want to modify my experiment's world, domain,
and simulation parameters so that I can define the problem I want to
simulate.

**Given** an experiment open in the editor
**When** I add, remove, or change objects and their physical properties, the
computational domain, simulation parameters, physics choices, or observation
requests
**Then** the experiment reflects the change, and only valid changes are
adopted.

**Acceptance criteria:**
- Authoring covers at minimum: objects and their physical properties
  (sources, masses, initial poses and velocities), the computational domain
  and its resolution, the simulation time step, which fields and physical
  models are simulated, initial conditions, and what is observed.
- Every change is validated before it is adopted. Invalid values — non-finite
  or out-of-range parameters, a time step that is numerically unstable for
  the active models, or contradictory physics choices — are rejected with an
  understandable reason, and the experiment keeps its last valid state.
- A committed change applies completely or not at all; a partially applied
  edit is never visible.
- UI actions, MCP tool calls, undo, and redo use the same authoritative edit
  path. Each proposed change is accepted as a new experiment revision or
  rejected with a reason; no adapter mutates the document directly.
- Externally initiated edits carry a stable command identity and actor
  provenance. A caller may guard an edit with the experiment revision it read;
  a duplicate is idempotent and a stale guarded edit is rejected explicitly.
- Edits affect only the editable experiment. They never mutate a workload
  that has been loaded or is running on a cluster; changing a submitted
  experiment means editing it and submitting again.
- Presentation state (camera, selection, visibility, window layout) never
  changes simulated values. A bounded default view may be saved in the file's
  separate presentation section without entering the experiment revision,
  undo history, or workload; changing its supported authoring subset advances
  the view revision and marks the file dirty.

### Compose a modeled object from plugin components

As a scientist-researcher, I want to compose an object's behaviour from
plugin-contributed components so that its visible configuration explains how
it participates in the simulation.

**Given** compatible simulation plugins are installed
**When** I create an object directly or instantiate a catalog template and add,
remove, or configure its components
**Then** Kagami validates one coherent composition and shows what each
component contributes.

**Acceptance criteria:**
- Every modeled object has stable identity, pose and initial velocity; an
  object with no executable physical component remains valid but is not
  silently simulated.
- An object without Dynamics is kinematic/static during a run. An object with a
  compatible Dynamics component is integrated; there is no separate persisted
  motion-authority switch.
- Component types, properties, dimensions, constraints and compatibility come
  from installed plugin schemas. Built-in and third-party components use the
  same authoring contract.
- A catalog template is the normal reusable way to instantiate a composition,
  but its template name, filename or particle species never selects hidden
  physics.
- Adding a dynamics component supplies inertial mass and opts the object into
  force/impulse integration. Position and velocity are intrinsic kinematic
  state; accumulated force and acceleration are run state or observations.
- Gravity, electrostatic and other field-coupling components contribute forces
  to dynamics. They do not independently advance the object's position.
- Inertial mass, gravitational mass and electric charge remain distinct
  dimensioned properties; any relationship between them is explicit and
  editable.
- An invalid or conflicting composition is refused atomically with component-
  specific diagnostics and creates no revision or partial object.

### Configure a particle emitter

As a scientist-researcher, I want an emitter to create particles from catalog
templates during a run so that I can model bounded streams and mixed
populations without authoring every particle individually.

**Given** an experiment contains compatible catalog templates and emitter
components
**When** I configure an emitter's spawn recipes, rate, capacity, direction,
spread, initial velocity, and optional lifetime
**Then** each run can create the selected composed objects reproducibly.

**Acceptance criteria:**
- An emitter is an ordinary modeled object. Adding dynamics or a field coupling
  makes the emitter itself move under the same rules as any other object.
- A spawn table may select multiple templates with explicit weights; the
  authoring command atomically materializes and persists their complete spawn
  blueprints and source fingerprints.
- Workload compilation revalidates and embeds those self-contained spawn
  blueprints and provenance without consulting the mutable catalog. A running
  Orishu workload never reads Kagami's catalog or a mutable template.
- Spawned particles are run state, not new experiment revisions or undo
  entries.
- Emission follows simulation time and a deterministic seed/profile, survives
  checkpoint/restart without duplication, and reports capacity exhaustion.
- Blueprint size, recipe count, rate, total/live particle capacity, lifetime
  and per-step work are bounded and validated before acceptance.

### Define variables and use expressions

As a scientist-researcher, I want to define named physical quantities and use
expressions in numeric fields so that relationships in my experiment remain
explicit, consistent, and reproducible.

**Given** an experiment open in the editor
**When** I define `mass_of_sun = 1e32 kg` and enter `mass_of_sun / 2` for an
expression-capable mass property
**Then** Kagami retains both expressions and resolves the property to a valid
mass through the experiment's variables and expressions subsystem.

**Acceptance criteria:**
- A plain numeric literal and arithmetic such as `1 / 3 + 0.1` are valid
  expressions for a dimensionless numeric field.
- Users can define, inspect, change, rename, and remove named variables with
  stable document-local identities, explicit namespaces/scopes, expressions,
  and optional descriptions.
- Expressions can reference other variables and supported mathematical or
  physical constants. Evaluation follows dependency order and detects cycles,
  unknown or ambiguous names, and invalid references.
- Physical literals support units. Arithmetic carries dimensions, and a value
  is accepted for a property only when its resulting dimension matches the
  property's schema; resolved domain values use canonical SI representation.
- The document persists authored expression source and variable definitions,
  not only their current floating-point results, and restores them without
  changing their meaning.
- Editing a variable evaluates its complete affected dependency closure. The
  edit and every dependent value are adopted as one document revision and undo
  entry, or all remain at their previous valid values with a diagnostic.
- Diagnostics identify syntax, name-resolution, dimension, cycle, division by
  zero, and non-finite failures and locate the relevant source span where
  possible.
- UI and MCP edits use identical parsing, evaluation, limits, diagnostics, and
  authoritative command decisions.
- Expression source and evaluation are bounded against excessive input length,
  nesting, graph size, dependency depth, and work.
- Submission preserves all definitions and expressions for the submitted
  experiment revision. Orishu validates and resolves the same shared language
  authoritatively before accepting and freezing the workload.
- Run observations never act as implicit variables. Using an observed value in
  authored intent requires an explicit adoption command with provenance.

### Create an object from the catalog

As a scientist-researcher, I want to create an object from a reusable catalog
template so that common physical compositions and parameter relationships do
not have to be rebuilt for every experiment.

**Given** an experiment is open and an object template is available
**When** I choose the template, supply its required parameters and placement,
and instantiate it
**Then** Kagami adds one complete, valid object to the experiment.

**Acceptance criteria:**
- The catalog presents named templates with descriptions, component/property
  schemas, parameters, expressions, units, availability, and diagnostics.
- Templates describe generic component/property composition. A template name,
  filename, or particle species never selects hidden solver behavior.
- Template parameters and properties use Kagami's shared variable/expression
  language. Instantiation resolves names, checks dimensions and schemas, and
  rejects the complete proposal if any value is invalid.
- Instantiation is a normal document command. It mints a stable object identity
  and produces exactly one experiment revision and undo entry, or changes
  nothing and reports a reason.
- The experiment persists the object's complete authored components,
  properties, expressions, and source provenance. It can be saved, opened,
  edited, and submitted without the source catalog.
- Changing, deleting, reloading, or losing a template never changes an
  existing object. Instantiating the changed template creates a new object;
  there is no tracking link or automatic/explicit propagation operation.
- A structurally valid template whose component/property schemas are not
  supplied by installed plugins remains visible with an `Unavailable`
  diagnostic and cannot be instantiated. Installing the compatible plugin
  revalidates the template without changing existing objects.

### Reuse a catalog value in an expression

As a scientist-researcher, I want to reference a specific property of a
catalog template in my own expression so that I can reuse a known physical
quantity, such as the Sun's mass, without instantiating that template as an
object just to read it.

**Given** an experiment is open and a catalog template defines an
expression-capable property
**When** I write an expression that references that property by its catalog
path, such as `planets.sun.mass / 2`, for one of my own variables or object
properties
**Then** Kagami resolves the reference through the shared variable system for
authoring, and compilation captures its complete resolved dependency closure
inside the immutable workload.

**Acceptance criteria:**
- Every expression-capable template property participates in a canonical
  catalog/template/component/property namespace without a separate export
  list. Public properties can be referenced from document expressions;
  private properties and helper variables remain visible only within their
  declared namespace.
- The short spelling `planets.sun.mass` works only when it is unambiguous. The
  resolved binding uses stable catalog, template, plugin/component, and
  property identities rather than editable display names.
- The document retains the catalog-qualified authored source. Reloading a
  catalog may update its derived preview or make it unavailable, but does not
  mutate the document or create an undo entry.
- A catalog expression may reference another visible catalog binding. Missing
  files, private dependencies, unknown plugin schemas, unresolved names,
  cycles, invalid dimensions, and non-finite values produce structured
  diagnostics; affected templates cannot be instantiated or compiled, while
  other catalog entries remain usable.
- Saving and reopening preserves an unresolved catalog-qualified expression
  without fabricating a value. It becomes valid again when the required
  catalog files and compatible plugins are available.
- Compiling an exact document revision captures the complete transitive
  catalog-variable closure from one immutable catalog snapshot. The workload
  retains rewritten expression sources, dimensions, canonical resolved values,
  source identities, and fingerprints, but no reference Orishu must resolve
  through Kagami's catalog.
- A catalog change after compilation never changes an accepted or running
  workload. Compiling again may produce a different workload and fingerprint.

### Manage the object catalog

As a scientist-researcher, I want to inspect and edit Kagami's object catalog
so that its reusable authoring vocabulary matches my work.

**Given** Kagami is running
**When** I create, change, remove, validate, or reload a template
**Then** the catalog authority decides the change and every catalog view sees
the resulting revision.

**Acceptance criteria:**
- The catalog is a versioned collection of bounded, human-readable files owned
  by the Kagami client, not by the open experiment or Orishu.
- Catalog files can be copied between users. Missing catalog dependencies or
  plugin-provided schemas are reported per entry rather than preventing the
  rest of the catalog from loading.
- UI and MCP operations use the same catalog commands, validation, revision,
  conflict detection, availability states, and diagnostics.
- One malformed or unsupported entry is isolated as invalid or unavailable;
  other entries and Kagami remain usable, and no scientific defaults are
  fabricated.
- Catalog edits do not mark the experiment modified. Instantiation changes the
  experiment through its document authority; catalog-variable previews are
  derived from the current catalog revision.
- Concurrent or external file changes cannot be silently overwritten; Kagami
  reports stale revisions or source fingerprints and requires a fresh edit.

### Validate and package a plugin

As a plugin author, I want Kagami's headless tooling to validate and package my
plugin source so that I do not have to calculate content identities or assemble
an installable bundle by hand.

**Given** a source manifest, declarative contribution schemas, and built
WebAssembly Components
**When** I run `kagami plugin validate` or `kagami plugin pack`
**Then** Kagami validates the public extension and workload contracts and
produces either structured diagnostics or a self-contained immutable release.

**Acceptance criteria:**
- The source manifest names logical identities, contributions, dependencies,
  compatibility, and local inputs; the author does not manually supply the
  release identity or descriptors for locally built artifacts.
- Validation is bounded and does not install, enable, or execute the plugin.
- Packing streams every declared artifact, records its digest, exact size and
  media type, canonically encodes the complete typed manifest, and derives the
  plugin release identity from it without a self-referential field.
- Full contribution identities are derived from the release identity,
  extension point, and plugin-local contribution identity.
- Repacking the same canonical manifest and artifact closure preserves the
  release identity even when archive compression or entry order differs.
- `--expect-release` lets automated builds fail when produced logical content
  differs from the expected release identity.
- Invalid extension points, malformed schemas, forbidden component imports,
  missing artifacts, excessive bounds, and digest conflicts produce structured
  diagnostics and no partially written bundle.
- Plugin commands run headlessly through the normal `kagami` executable and do
  not open the authoring window.
- Initial plugins contribute scientific schemas and optional bounded
  presentation annotations only. Kagami renders them through host-owned generic
  controls and visualizers; the bundle cannot add views, windows, renderers,
  widgets, command handlers, or executable UI.

### Add a plugin to Kagami

As a plugin author, I want to add a plugin I built with external tooling to
Kagami so that experiments can model physical phenomena beyond the built-in
set.

**Given** Kagami is running with its built-in plugins (for example
electrodynamics and gravity)
**When** I add a plugin — its manifest and executable component artifacts
**Then** Kagami validates it against the plugin and workload contracts and
makes its phenomenon available as a physics choice for experiments.

**Acceptance criteria:**
- Kagami ships with predefined plugin releases validated through the same
  public contract as imported releases. They are installed and enabled by
  default, not privileged or impossible to disable.
- A plugin is added as one isolated, manifest-driven bundle containing one or
  more extension-point contributions and content-addressed workload components.
  Files do not register capabilities by occupying a shared path.
- Kagami independently recomputes the release identity and validates the
  manifest and components against the [workload contract](../../protocol-workload.md) —
  engine, lifecycle, component world, declared imports, and limits — before
  atomically installing the release.
- A structurally invalid release is rejected with structured diagnostics and no
  partial installation. An unsupported contribution is retained but dormant;
  it and its declared dependents do not prevent understood independent
  contributions from the same release becoming available.
- Adding a plugin never mutates an open experiment and is not part of the
  saved experiment document; experiments reference exact provider-qualified
  contribution and release identities.
- Availability is calculated per contribution. Missing scientific-contract
  dependencies may leave one contribution dormant while unrelated
  contributions from the same valid release remain available.
- A contribution appears in the appropriate authoring choices only when its
  bounded transitive dependency closure is available.
- Updating installs a new immutable release side-by-side and makes it the
  default for new authoring; experiments keep referencing the exact release
  they were authored against unless explicitly migrated.
- Removing a plugin from Kagami never changes experiments that reference it;
  they keep their authored release and contribution identities and report the
  unavailable capability.

### Share a plugin

As a plugin author, I want to share a plugin with other Kagami users so that
they can model the same phenomenon without rebuilding it.

**Given** a validated plugin added to Kagami
**When** I share it as a self-contained artifact
**Then** another Kagami instance can add and use the same plugin.

**Acceptance criteria:**
- A plugin is shareable as a self-contained artifact carrying its manifest and
  declared artifacts; the recipient does not need the author's build
  environment or Kagami installation.
- The recipient adds the shared plugin through the normal add flow with the
  same validation and diagnostics.
- Sharing never changes experiments that already reference the plugin; they
  keep their content-addressed plugin identity.
- The shared artifact preserves the plugin's identity: adding it elsewhere
  produces the same content-addressed plugin.

### Inspect and manage installed plugin releases

As a researcher or plugin author, I want to inspect and manage immutable plugin
releases so that updates are understandable and do not silently change existing
experiments.

**Given** Kagami has bundled or imported plugin releases
**When** I list, inspect, install, update, choose a default, or remove a release
**Then** Kagami reports and changes the plugin inventory without editing any
experiment.

**Acceptance criteria:**
- `kagami plugin list --all-releases` distinguishes logical plugin identity,
  every installed release, the default for new authoring, origin, persistent
  enablement, and contribution-level availability.
- `kagami plugin inspect` reports exact contribution identities, scientific
  contracts and dependencies, artifact descriptors, compatibility, and
  structured diagnostics without executing component code.
- Installing an already present identical release is idempotent. The same
  logical plugin may retain several immutable releases side-by-side.
- `kagami plugin update` installs another release; it never modifies or removes
  existing release contents. The remote source and publisher-trust policy are
  defined separately before network update is implemented.
- `kagami plugin set-default` affects only future authoring selections. It does
  not migrate open or saved experiments.
- Removal names one exact release, warns about known open-document references,
  and cannot claim to discover every experiment file on disk. Removing it does
  not rewrite those files.
- Inventory commands and the future UI and MCP adapters command the same
  authority and observe the same inventory revision, states, and diagnostics.

### Enable or disable a plugin

As a researcher, I want to disable a logical plugin persistently or only for one
Kagami invocation so that I can control the available extension vocabulary
without uninstalling releases or changing experiments.

**Given** one or more releases of a logical plugin are installed
**When** I enable or disable that plugin, or start Kagami with a process-only
override
**Then** all of that plugin's contributions participate in or are suppressed
from availability resolution for the selected scope.

**Acceptance criteria:**
- `kagami plugin enable` and `kagami plugin disable` change persistent
  user-owned enablement, not an experiment or plugin release.
- `kagami --enable-plugin` and `--disable-plugin` override persistent
  enablement only for that process and do not modify configuration.
- Disabling applies to the logical plugin and all its installed releases.
  Enabling uses the default release for new authoring while exact older
  releases remain usable by experiments already pinned to them.
- A disabled contribution is reported as disabled, not missing, corrupt, or
  scientifically incompatible.
- Re-enabling recomputes contribution availability without reinstalling the
  release or revising an experiment.
- Bundled and imported plugins obey the same enablement path, and Kagami remains
  usable with every simulation plugin disabled.

### Author an experiment with plugin-contributed models

As a scientist-researcher, I want to choose plugin-contributed computational
models so that I can simulate fields beyond the built-in set.

**Given** an experiment open in the editor and at least one available plugin
**When** I select a model for a field family
**Then** the experiment records the stable plugin/model/schema identities and
submission or export includes its executable code in the workload closure.

**Acceptance criteria:**
- The model choice lists models from built-in and added custom plugins with
  their field family, availability, compatibility and diagnostics.
- Selecting a model is a normal validated experiment edit: one revision and
  undo entry, or a rejection with a reason.
- The experiment records field-family, plugin, model and schema identities. The
  selected executable code is part of the workload closure at submission/export;
  the experiment remains reproducible without the plugin's source or the
  Kagami installation that added it.
- Compilation follows the bounded transitive closure of selected contributions
  across plugin releases. It includes every required scientific contract,
  schema and executable artifact, but excludes unselected kernels and unrelated
  contributions from those same bundles.
- If a referenced plugin artifact is missing at submission, Kagami reports
  what is missing rather than substituting another plugin.

### Modify an experiment through MCP

As a scientist-researcher using an external agent, I want the agent to propose
experiment edits through Kagami's MCP server so that automated and interactive
authoring obey the same document rules.

**Given** an experiment is open and Kagami's MCP server is enabled
**When** an authenticated MCP client invokes an authoring tool
**Then** Kagami decides the corresponding experiment command and reports its
accepted or rejected result.

**Acceptance criteria:**
- MCP tools expose domain authoring operations, not mutable document storage or
  Kagami's internal scene representation.
- An MCP edit uses the same validation, atomic commit, revision, dirty-state,
  identity, and undo path as the equivalent UI edit.
- Every MCP command has a stable command identity and actor provenance. Retrying
  an already-decided identity is idempotent and returns the original decision.
- An MCP client may name the experiment revision on which its proposal is
  based. If that revision is stale, Kagami rejects the command with the current
  revision rather than silently applying it to different state.
- The tool result distinguishes successful delivery from document acceptance:
  acceptance reports the resulting revision; rejection reports a domain reason
  and leaves the experiment unchanged.
- MCP observation and presentation tools cannot bypass the authoring command
  path or turn run observations into document edits.
- Server lifecycle — enabling and disabling the MCP server, seeing connected
  clients — and the wider parity surface (inspection, document lifecycle, run
  control) are captured in [External control through MCP](./mcp.md).

### Undo and redo modifications

As a scientist-researcher, I want to undo and redo modifications to my
experiment so that I can correct authoring mistakes without rebuilding state.

**Given** an experiment with an edit history
**When** I undo (Ctrl+Z / Edit > Undo) or redo (Ctrl+Y / Edit > Redo)
**Then** the experiment returns to the preceding or following authored state.

**Acceptance criteria:**
- Undo and redo are available through standard keyboard shortcuts and the
  Edit menu.
- The undo and redo controls indicate whether they are available and name the
  change they would reverse or reapply.
- Undo and redo apply to authored experiment changes (world, domain,
  parameters); they never alter computed states, stored results, or a
  cluster's loaded workload.
- One interactive gesture (for example, dragging an object to a new position)
  counts as one undo step, regardless of how many intermediate values it
  passed through.
- Restoring a previous state is validated like any other edit: if the current
  experiment configuration cannot represent it, the restore is refused with a
  reason and the history is left unchanged.
- Undo preserves identity: undoing a removal restores the object with the
  same identifier and attachments, never a replacement.
- Undo and redo are available only in Authoring mode. Observation/replay mode
  does not display them, because it presents immutable run output rather than
  the editable initial scene; a run boundary never becomes an undo entry.
- Choosing **Edit initial conditions** explicitly returns to Authoring and the
  authored initial scene, where undo/redo are available again. It stops a local
  preview, but merely detaches from a remote run unless the user separately
  invokes the privileged stop action.
- Redo is available only after an undo; a new edit clears the redo branch.

## Observing the experiment

### Add and attach a probe

As a scientist-researcher, I want to add a non-perturbing probe at a fixed
position or attached to a modeled object so that a run records scientific
values at the location I care about.

**Given** an experiment and plugin-declared observation channels
**When** I add a probe, choose channels, and optionally attach it to an object
with a local offset
**Then** the observation request follows the object without changing its
physics.

**Acceptance criteria:**
- The world and scene tree distinguish modeled objects, which may affect and
  evolve in the simulation, from observation instruments, which only request
  or visualize values.
- Modeled simulation state includes particles/objects and selected fields:
  particles may move and fields may evolve, while instruments never drive
  either evolution.
- A probe has stable identity and either a world pose or an object attachment
  plus local offset. Renaming the object preserves the attachment; deleting it
  is refused until the dependency is cleared in the same edit.
- Probe channels use stable plugin-declared identities with dimensions and
  validity semantics. A temporarily unavailable channel is retained and shown
  unavailable rather than discarded.
- Probes and attached sampling regions never carry physical components or
  contribute forces. A detector intended to perturb the experiment must be
  modeled explicitly as a composed object.
- Recording cadence and retention are bounded workload/output requests;
  viewport visibility and transport subscription density remain client-local.

### Select fields and their computational models

As a scientist-researcher, I want to define a simulation domain and select the
fields and computational models active in it so that the experiment states both
what exists throughout space and how it evolves.

**Given** installed plugins contribute compatible field and model schemas
**When** I select field families and one model for each family
**Then** Kagami validates an explicit, executable combination.

**Acceptance criteria:**
- A selected field is conceptually defined at every point of the authored
  domain; its mesh, basis, particles, cells, or other numerical representation
  is declared by the chosen model rather than mistaken for the field itself.
- Exactly one model evolves each selected field family. Coulomb and Maxwell/Yee
  are alternative electromagnetic models and cannot both own that family;
  classical gravity and GEM are alternative gravitational models.
- Alternative models can use stable coupling properties such as electric
  charge or gravitational mass, so changing model does not silently reinterpret
  an object's authored composition.
- Compatible families may coexist. Hydrodynamic models may treat a real medium
  and its flow as field state over the same domain.
- Each model comes from a plugin's declarative schemas and pinned executable
  update code. Switching model is an explicit validated edit and unavailable or
  incompatible models are never replaced implicitly.

### Switch projection and follow an object

As a scientist-researcher, I want to switch between perspective and
orthographic projection and optionally have the camera follow an object so that
I can inspect the scene from a useful, stable frame of reference.

**Given** an experiment or run is visible
**When** I choose a projection in the view control or select an object to
follow
**Then** the current window updates without changing scientific experiment
intent or run state.

**Acceptance criteria:**
- The view control clearly exposes Perspective and Orthographic projection and
  reports the active choice.
- In Authoring mode, projection, camera pose, orbit/focus and supported follow
  settings update the file's default-view revision, mark the file modified, and
  are restored on reopen without changing experiment revision, undo, or workload.
- In Observation/replay mode, camera and projection changes are ephemeral and
  never dirty the experiment file or alter a result/run reference.
- A camera may follow a modeled object's authoritative observed pose while the
  run evolves; the user can stop following without changing that object.
- If a followed object is absent from the current experiment or observation,
  Kagami reports that condition and safely releases the follow target.
- Camera movement and following are per-window presentation. They do not affect
  another observer and are not controllable through the shared MCP surface.

### Choose the scene scale

Implementation: [K14 — Choose the scene scale](../../tasks/kagami/choose-scene-scale.md)
(K-VIEW, Milestone 2; specified, not implemented).

As a scientist-researcher, I want to say how many metres one viewport unit
represents so that I can work on an atomic-scale or an astronomical-scale
experiment through the same camera.

**Given** an experiment or run is visible
**When** I choose a scale preset in the view control, or enter a distance per
unit directly
**Then** the viewport reaches and frames the scene at that scale, without
changing any stored position, size, or physical constant.

**Acceptance criteria:**
- The view control offers named presets spanning at least nanometre to
  light-year, reports the active one, and accepts a directly entered
  distance-per-unit — including a unit-bearing entry such as `1 nm`. A value
  matching no preset is shown as a custom scale rather than silently rounded to
  the nearest one.
- The scale must be a positive, finite length within the supported range,
  which includes all presets. Invalid, out-of-range and wrong-dimension
  entries are refused with a reason and leave the current view unchanged.
- Camera reach follows the scale. At nanometre scale the camera approaches a
  nanometre-sized object instead of stopping short of it; at astronomical scale
  it pulls back far enough to frame an orbit. Zoom, pan and focus limits are
  expressed relative to the scale rather than as fixed distances in metres.
- Changing scale preserves the focus point, orientation and projection. Camera
  distance is preserved when valid at the new scale; any adjustment to a new
  distance limit is reported. If the focus cannot be represented safely at that
  scale, the change is refused with guidance to retarget the view first.
- Changing the scale never changes an authored position, extent, velocity,
  expression or constant, never advances the experiment revision, and never
  enters document undo. It is not a numerical-domain setting.
- Where the scale seeds a *default* for new authoring — the extent an added
  object starts with — that default becomes ordinary authored intent at the
  moment the object is created, and later scale changes do not revisit it.
- In Authoring mode the scale is part of the saved opening view: it updates the
  default-view revision, marks the file modified, and is restored on reopen. In
  Observation/replay mode scale changes are ephemeral, exactly as camera and
  projection changes are.
- The scale is visible, not merely in effect: the viewport states the active
  scale, and distances the viewport reports are in real units regardless of it.
- Scale is per-window presentation. It does not affect another observer, does
  not travel with a run reference, and is not controllable through the shared
  MCP surface.
- Objects, fields and trails at radically different magnitudes in one
  experiment remain individually inspectable by changing scale; no single scale
  is required to make the whole scene legible at once.

**Delivery boundary:** K14 supplies the scale and camera/conversion contract;
object, field and trail rendering consume it as their own tasks land. Uniform
scaling alone cannot preserve arbitrarily small detail far from the world
origin. That part of the broader inspection outcome needs follow-up
camera-relative precision work in K-VIEW and is not claimed by K14.

### Visualize a vector field

As a scientist-researcher, I want to display a field with vectors or flow lines
so that its direction, magnitude and structure are understandable in space.

**Given** a run supplies a compatible field observation over a region
**When** I enable vector glyphs or flow lines and choose their client-side
density and styling
**Then** Kagami visualizes only values supported by that observation.

**Acceptance criteria:**
- Vector and flow-line layers consume dimensioned field samples carrying run,
  boundary, model, precision, completeness and validity provenance.
- Singular, outside-domain, unavailable and stale samples are distinguished
  from zero; a flow line stops or is marked when no valid continuation exists.
- Density, seeding, color, scale and visibility are bounded presentation
  choices and do not alter requested observations, solver state or another
  observer's view.
- Interpolation used for display is labeled presentation-only and can never
  become a result, checkpoint, force, or authored value implicitly.

### Show live trails and recorded trajectories

As a scientist-researcher, I want to show a bounded history of particle
positions so that motion is apparent while I interact with a live or replayed
run.

**Given** object-position observations are available
**When** I enable trails and choose a duration or sample limit
**Then** Kagami draws each selected object's recorded path through simulation
time.

**Acceptance criteria:**
- A live trail may be a best-effort client history of authoritative boundaries;
  dropped/coalesced observations create visible gaps rather than invented path.
- An exact trajectory is an authored, retained observation request and is
  queryable during replay or through MCP with coverage and provenance.
- Neither form is derived from render-frame positions or unlabelled
  interpolation/extrapolation.
- History duration, selected object count, samples and GPU/CPU memory are
  bounded; overload degrades explicitly without affecting the run.
- Seeking or changing run identity rebuilds or clears incompatible trail
  history rather than joining unrelated paths.
- Trail visibility and styling are client-local, are not physical state, and
  are not required for the first essential field-visualization slice.

## Running simulations

### Switch between authoring and observation

As a scientist-researcher, I want Kagami to state whether I am editing initial
conditions or observing a run so that I never mistake playback for an editable
experiment.

**Given** an experiment is open or a run is selected
**When** I submit/start a run or choose Edit initial conditions
**Then** Kagami switches explicitly between Observation/replay and Authoring.

**Acceptance criteria:**
- Submitting or opening a run enters Observation/replay and prominently shows
  the run identity, simulation time and playback state.
- Observation/replay exposes run controls and visualization but no document
  mutation, undo or redo controls. MCP authoring is gated by the same mode.
- Edit initial conditions returns to the initial authored scene and makes
  document controls available; it never adopts a displayed computed state.
- Leaving a local preview stops it. Leaving a remote run detaches the window and
  does not stop cluster execution unless the user separately confirms that
  privileged action.
- Opening or creating an experiment starts in Authoring and never resumes a run
  from saved presentation data.
- An MCP client must request the transition to Authoring explicitly before an
  edit; a rejected edit cannot switch modes as a hidden side effect.

### Run an experiment locally

As a scientist-researcher, I want to run my experiment locally so that I can
verify its behavior before committing cluster resources to it.

**Given** a valid experiment open in Kagami
**When** I start a local run
**Then** Kagami advances the simulation in-process and shows the computed
states.

**Acceptance criteria:**
- A local run executes an immutable input compiled from a specific experiment
  revision, advancing it with fixed time steps using the same physics the
  experiment would use when submitted to a cluster.
- Local preview executes the same immutable component graph, step plan and
  lifecycles the cluster would run, where the supported profile allows it; it
  never silently grants capabilities that remote execution denies.
- Play, pause, and single-step controls are available; stepping advances the
  simulation by exactly one accepted step and then pauses.
- Local and cluster runs publish the same observation semantics: values,
  units, validity, and provenance mean the same thing in both, so a locally
  verified experiment behaves compatibly when submitted.
- Every rendered value identifies the experiment and simulation time that
  produced it.
- Simulation time advances only through accepted fixed steps; a playback
  speed control may change viewing pace but never changes the time step or
  the results.
- A local run remains tied to the experiment revision from which it was
  compiled. Edits made while it is active affect only the document; observing
  those edits in simulation requires starting a new run from the new revision.
- The visualization consumes produced observations only; it never reads
  solver-owned memory directly.
- Local observations update only the run projection. They never modify the
  experiment or mark its document dirty.
- Local run output is preview output: it is not stored as cluster result or
  checkpoint artifacts.

### Connect to an Orishu cluster

As a scientist-researcher, I want to connect Kagami to an Orishu cluster so
that I can use it as distributed compute for my experiments.

**Given** Kagami is running and a cluster endpoint is reachable
**When** I select the endpoint and authenticate
**Then** Kagami establishes an authenticated client session and shows the
cluster's state.

**Acceptance criteria:**
- I can specify a cluster endpoint (for example, host and port); Kagami
  indicates whether it is connecting, connected, disconnected, or failed.
- Remote connections authenticate with credentials (such as a token or
  certificate); authentication failure is reported with a clear error.
- Once connected, Kagami shows enough cluster state to act on: whether a
  workload is loaded, and in which state it is.
- Kagami acts only as an authenticated client: it never joins cluster
  membership or participates in peer coordination.
- If the connection is lost, Kagami shows the session as disconnected, keeps
  the last known cluster state clearly labeled as stale, and allows
  reconnecting.
- Connection problems never modify the open experiment.

### Submit an experiment to a cluster

As a scientist-researcher, I want to submit my experiment to the connected
cluster so that it runs as a distributed workload.

**Given** a valid experiment and a connected cluster
**When** I submit the experiment
**Then** the cluster accepts it as an immutable workload and prepares
execution.

**Acceptance criteria:**
- Submission compiles the experiment into the shared workload contract: one
  canonical manifest plus the complete closure of digest-addressed workload
  component and input artifacts. Kagami and Orishu use the same public types
  and canonical codec rather than translating between application schemas.
- Kagami may send that workload through any cluster-supported distribution
  format. Thin transfer, a portable bundle, cache reuse, or peer retrieval do
  not change workload identity, and only missing blobs need be transferred.
- A portable bundle for offline sharing or archival can be produced without
  submitting (see
  [Export an experiment as a portable bundle](#export-an-experiment-as-a-portable-bundle)).
- Submission reports the outcome: accepted with the resulting workload
  identity and run reference, or rejected with a reason.
- Before submission, Kagami checks that the experiment is complete and valid;
  an incomplete experiment is not submitted, and what is missing is
  explained.
- Submission names the exact experiment revision from which Kagami compiled the
  workload. Edits accepted after compilation are successor draft revisions and
  cannot enter that submission implicitly.
- Where the cluster supports a side-effect-free compatibility check, Kagami
  reports whether the cluster can run the experiment before anything is
  replaced.
- If the cluster already has an active workload, submission requires my
  explicit confirmation, because loading my experiment gracefully stops and
  replaces it.
- After acceptance, the cluster is authoritative for the workload: later
  edits to my experiment in Kagami do not mutate the loaded workload.
- Submission is a privileged operation: it requires authenticated access, and
  insufficient credentials are reported as a clear error rather than a silent
  failure.
- A failed submission leaves my open experiment intact.

### Export an experiment as a portable bundle

As a scientist-researcher, I want to export my experiment as a portable bundle
so that I can share or archive a complete, runnable workload without operating
an artifact repository.

**Given** a valid experiment
**When** I export it (File > Export...)
**Then** Kagami compiles the experiment into a workload and writes one
portable bundle carrying the manifest and complete artifact closure.

**Acceptance criteria:**
- Export is available from the File menu (File > Export...) and produces one
  deterministic file.
- The bundle carries the workload manifest and the complete closure of
  content-addressed artifacts: the plugin kernels for the phenomena the
  experiment models, initial conditions, and any other required inputs.
- The bundle is self-contained: Orishu can load and run it independent of the
  Kagami instance that produced it — without the source experiment document,
  the object catalog, or the plugin source.
- Export produces the immutable workload bundle, not the editable experiment
  document; sharing the editable draft remains a file-sharing operation (see
  [Share an experiment draft](#share-an-experiment-draft)).
- Exporting never changes the experiment, never submits anything to a cluster,
  and never starts a run.
- The bundle is transport, not identity: importing it elsewhere preserves the
  workload's root and artifact digests, and thin transfer, cache reuse, or
  peer retrieval of the same workload remain interchangeable.
- Export is available for experiments that use custom plugins as well as
  built-in ones.

### Control and observe a cluster run

As a scientist-researcher, I want to control my submitted workload and watch
its progress from Kagami so that I can verify it evolves correctly.

**Given** my experiment has been submitted to a connected cluster
**When** I start, pause, step, or stop the run and watch its live output
**Then** the cluster run responds to my controls and I see committed
simulation states as they are produced.

**Acceptance criteria:**
- Run controls are available in Kagami — start, pause/stop, and step — with
  the same semantics as the cluster's workload controls (see
  [orishu workload stories](../orishu/workload.md)).
- Kagami shows workload state and progress: loading, ready, running, stopped,
  or error, along with simulation time.
- Live observations stream into Kagami as the cluster commits simulation
  boundaries; every rendered value carries workload, simulation time, and
  validity provenance.
- Cluster observations update only the selected run projection. They never
  modify the open experiment, including when they are newer than its submitted
  revision.
- Observing is scientifically read-only. Bounded subscription, transport, and
  rendering work is isolated and may degrade or disconnect, but it cannot alter
  scientific state, enter the step decision, or block simulation commit.
- If the connection drops, the last received observation may remain visible
  but is clearly labeled stale; Kagami never mixes data from different
  observations.
- Controls that change the run require privileged access; read-only
  observation follows the cluster's read-access rules.

### Observe a run shared by another user

As a collaborator, I want to open a shared run reference so that I can watch
the same simulation without sharing the submitter's Kagami session or being
granted run-control authority.

**Given** an accessible Orishu cluster and a run reference
**When** I open the run in Kagami
**Then** Kagami independently subscribes to its live or persisted observations.

**Acceptance criteria:**
- The reference identifies the cluster, immutable workload, and workload epoch
  sufficiently to reject accidental attachment to a replacement run.
- Multiple Kagami clients can observe the same run concurrently without one
  client relaying observations to the others.
- Each observer independently chooses channels, spatial region, level of
  detail, camera, selection, live-follow state, playback cursor, and playback
  rate. These choices do not affect the simulation or another observer.
- A live observer may follow the newest committed boundary, pause locally, or
  seek into persisted output when available.
- Joining, disconnecting, seeking, changing playback rate, and reconnecting are
  read-only. They require observation access but not submission or run-control
  capability.
- Live reconnection follows the observation-baseline rules in
  [ADR 0011](../../adr/0011-classify-network-flows-and-baseline-observation-deltas.md);
  historical playback never substitutes an incomplete or
  presentation-predicted frame for stored scientific output.
- If persisted observations do not cover a requested simulation time, Kagami
  reports the unavailable range rather than fabricating continuity.

### Adopt a computed state into an experiment

As a scientist-researcher, I want to adopt a selected computed state as new
experiment intent so that I can continue authoring from a simulation result
without silently changing the source document.

**Given** a complete observation from a local or cluster run
**When** I explicitly adopt supported values, such as object state at a chosen
simulation boundary, as new initial conditions
**Then** Kagami proposes and validates a normal experiment edit that creates a
new revision.

**Acceptance criteria:**
- Receiving, displaying, or selecting an observation never performs adoption;
  the user or an authorized MCP client must request it explicitly.
- Adoption names the source workload, run, simulation boundary, and observation
  identity and retains that provenance in the resulting authored state.
- Adoption is atomic and validated against the current experiment. Failure
  leaves the document and its history unchanged, reports a domain reason, and
  keeps the user in Observation/replay mode.
- A successful adoption marks the experiment modified, creates one undo entry,
  switches to Authoring mode, and does not mutate the source run, workload,
  observation, or artifact. It stops the owned local preview, but only detaches
  from a remote run unless the user separately has and invokes stop authority.
- An adoption guarded by a stale base revision is rejected under the same
  concurrency rules as any other externally initiated edit.

## Reviewing previous runs

### Open a previous run

As a scientist-researcher, I want to open a previous simulation run so that I
can review what was computed together with the experiment that produced it.

**Given** a connected cluster with stored runs, or a saved run file
**When** I list the available runs and open one
**Then** Kagami presents the run's computed states together with the context
that produced them.

**Acceptance criteria:**
- Kagami lists available runs with enough metadata to choose between them:
  the workload/experiment identity, when the run was recorded, and the
  simulation-time range it covers.
- Opening a run restores both entities: the experiment context (what was
  simulated) and the computed states (the stream of modeled states).
- I can review computed states after the fact — selecting a point in
  simulation time and moving through the run like a recording — whether the
  states are streamed from the cluster or loaded from a file.
- Every displayed value retains its provenance: workload and run identity,
  simulation time, model, precision, and validity.
- Opening and reviewing a run is read-only: it never mutates stored
  artifacts, and never affects a simulation that is currently running.
- An incomplete or incompatible run is reported clearly rather than partially
  or silently misrepresented.

## Follow-ups

These stories remain deliberately deferred:

- Detailed slice-plane/volume authoring, selection and manipulation gizmos,
  and viewport navigation beyond projection, scene scale and object following.
- Comparing observations across runs and exporting selected results.
- Live multi-writer experiment authoring, shared presence, presenter-follow
  mode, and synchronized playback.
