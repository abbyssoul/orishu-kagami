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

- **Experiment** — the editable intent a user crafts: the world (objects and
  their physical properties), the computational domain and discretization,
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
- The document persists experiment intent only: computed states, results, and
  run history are not part of an experiment document.
- After a successful save the experiment is marked unmodified; later edits
  mark it modified again. The file name and modified state are visible in the
  window (for example, in the title).
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
- The file contains editable intent only and does not silently carry a live
  cluster connection, run-control capability, presentation state, credentials,
  or observation cache.
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
  changes simulated values and is not part of the saved experiment document.

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

### Add a plugin to Kagami

As a plugin author, I want to add a plugin I built with external tooling to
Kagami so that experiments can model physical phenomena beyond the built-in
set.

**Given** Kagami is running with its built-in plugins (for example
electrodynamics and gravity)
**When** I add a plugin — its manifest and its kernel
**Then** Kagami validates it against the plugin and workload contracts and
makes its phenomenon available as a physics choice for experiments.

**Acceptance criteria:**
- Kagami ships with a pre-defined set of plugins; they are always available
  for experiments.
- A plugin is added as a manifest plus a content-addressed workload component
  (its kernel). The manifest names the plugin and describes the phenomenon it
  models, including the variables it exports. Kagami validates the manifest
  and the kernel against the [workload contract](../../protocol-workload.md) —
  engine, lifecycle, component world, declared imports, and limits — before
  the plugin becomes available.
- An invalid or unsupported plugin is reported as unavailable with structured
  diagnostics; it does not affect other plugins or Kagami itself, and no
  scientific defaults are fabricated.
- Adding a plugin never mutates an open experiment and is not part of the
  saved experiment document; experiments reference plugins by
  content-addressed identity.
- A plugin that passes validation appears in the experiment's physics choices
  with its declared phenomenon and exported variables.
- Updating a plugin produces a new artifact identity; experiments keep
  referencing the plugin they were authored against unless explicitly changed.
- Removing a plugin from Kagami never changes experiments that reference it;
  they keep their authored plugin identity and report missing artifacts at
  submission.

### Share a plugin

As a plugin author, I want to share a plugin with other Kagami users so that
they can model the same phenomenon without rebuilding it.

**Given** a validated plugin added to Kagami
**When** I share it as a self-contained artifact
**Then** another Kagami instance can add and use the same plugin.

**Acceptance criteria:**
- A plugin is shareable as a self-contained artifact carrying its manifest and
  kernel; the recipient does not need the author's build environment or
  Kagami installation.
- The recipient adds the shared plugin through the normal add flow with the
  same validation and diagnostics.
- Sharing never changes experiments that already reference the plugin; they
  keep their content-addressed plugin identity.
- The shared artifact preserves the plugin's identity: adding it elsewhere
  produces the same content-addressed plugin.

### Author an experiment with a plugin

As a scientist-researcher, I want to choose which plugin's phenomenon my
experiment models so that I can simulate fields beyond the built-in set.

**Given** an experiment open in the editor and at least one available plugin
**When** I select a plugin for the experiment's physics
**Then** the experiment records the modeled phenomenon and that plugin's
kernel, and submission or export includes the kernel in the workload closure.

**Acceptance criteria:**
- The experiment's physics choice lists available plugins — built-in and added
  custom plugins — with their availability and diagnostics.
- Selecting a plugin is a normal validated experiment edit: one revision and
  undo entry, or a rejection with a reason.
- The experiment records which phenomenon it models by plugin identity. The
  plugin's kernel is part of the workload closure at submission and export;
  the experiment remains reproducible without the plugin's source or the
  Kagami installation that added it.
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
- Undo and redo are refused while a simulation is running. Solver motion is
  run state and never changes authored objects, so merely running or observing
  a simulation neither adds nor discards document history.
- Redo is available only after an undo; a new edit clears the redo branch.

## Running simulations

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
- Local preview executes the same plugin kernel (workload component) and
  lifecycle the cluster would run, where the supported profile allows it; it
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
- Observing is read-only: connecting, watching, or disconnecting never
  changes the simulation, its performance, or its stored artifacts.
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
  leaves the document and its history unchanged and reports a domain reason.
- A successful adoption marks the experiment modified, creates one undo entry,
  and does not mutate the source run, workload, observation, or artifact.
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

These stories are deliberately deferred from the initial set and should be
captured as separate stories once the experiment model exists:

- Detailed per-entity authoring: objects and physical components, probes,
  slice planes, and per-field model selection beyond the plugin choice
  (compare Field CAD's world, measurement, and field-system stories).
- Viewport navigation and presentation controls (camera, selection, gizmos,
  display layers) as client-local stories.
- Comparing observations across runs and exporting selected results.
- Live multi-writer experiment authoring, shared presence, presenter-follow
  mode, and synchronized playback.
