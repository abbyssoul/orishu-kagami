# User stories: External control of Kagami through MCP

User stories in this file are written from the perspective of a
scientist-researcher who delegates parts of their Kagami session to an
external client — an AI agent, a script, or another application such as a web
front-end — that commands the running Kagami app through its MCP server. The
external client is an alternative interface to the same session: it authors
the same experiment, controls the same runs, and reads the same observations
that the scientist-researcher sees in the window. Nothing in these stories
gives an MCP client a direct relationship with an Orishu cluster: the Kagami
process remains that MCP client's only Orishu path. Other Kagami instances may
independently connect to the same run under the shared-observation stories.

This is local delegated control, not ADR 0012's future remote collaboration
service. Exposing the embedded MCP listener beyond the machine would not by
itself define collaborator identity, permissions, shared undo, persistence, or
document reconnect semantics.

## The parity guarantee

Kagami's MCP surface provides parity of *experiment meaning*, not parity of
mouse gestures (see [ADR 0006](../../adr/0006-mcp-ui-equivalence.md)):

- What an external client can do is bounded by what the UI can do: every
  authoring operation, document operation, run control, and read the UI
  offers is expressible through MCP, and MCP never bypasses a rule the UI
  obeys.
- MCP commands enter the same authorities as UI actions. Authoring goes
  through the document authority (see
  [Modify an experiment through MCP](./authoring.md#modify-an-experiment-through-mcp));
  run control goes through the same run controls the UI uses; reads return the
  same observations with the same provenance.
- Presentation state — camera, selection, visibility, window layout — belongs
  to the Kagami window. It is not part of the shared MCP surface, and an MCP
  client cannot move the user's view.
- The external client shares the live session. An agent and a user working at
  the same time see one experiment and one set of runs; the document
  authority's revision and command-identity rules resolve their concurrency.
- Every MCP request is untrusted input: authenticated, bounded, and validated
  before it touches experiment or run state. A malformed or unauthorized
  request fails with a clear result and never crashes or hangs the app.

## MCP server lifecycle

### Enable the MCP server in the running app

As a scientist-researcher, I want to enable Kagami's MCP server from the UI so
that I can let an external client work on the exact session I have open,
without restarting Kagami.

**Given** Kagami is running with its MCP server disabled
**When** I enable the MCP server
**Then** Kagami starts the server and shows me what a client needs to
connect.

**Acceptance criteria:**
- The MCP server is off by default; enabling it is my explicit action in the
  running app.
- Once enabled, Kagami shows the server endpoint and a freshly generated
  credential; a client must present the credential with every request, on any
  transport.
- The credential is shown masked by default with an explicit way to copy it,
  and is never written to disk or into the experiment document; disabling and
  re-enabling later generates a new credential.
- Enabling the server never modifies the open experiment, a run, or a cluster
  connection, and server state is not part of the saved experiment document.
- The server listens only on local transports (loopback or same-host IPC);
  enabling MCP never exposes authoring or run control to other machines.
- If the server cannot start (for example: the endpoint is already in use),
  Kagami reports the failure with a reason, leaves MCP disabled, and the app
  remains fully usable.

### See MCP server status and connected clients

As a scientist-researcher, I want to see whether the MCP server is enabled and
how many clients are connected so that I know whether an external client can
reach my session, and can verify that a new client connected.

**Given** Kagami is running
**When** I look at the MCP status in the UI
**Then** Kagami shows the server's state and its connected clients.

**Acceptance criteria:**
- The status distinguishes at least: server disabled, server enabled (with its
  endpoint), and server failed to start.
- While the server is enabled, the status shows how many MCP clients currently
  hold a session, including the explicit zero case.
- The count reflects sessions the server knows about; a client that vanished
  without closing its session may remain counted until the server notices, and
  the status does not claim more freshness than that.
- Reading the status never changes server state, the experiment, or any run.

### Disable the MCP server

As a scientist-researcher, I want to disable the MCP server so that no
external client can keep commanding my session after I am done delegating.

**Given** the MCP server is enabled, possibly with clients connected
**When** I disable it
**Then** Kagami stops external access, and I understand what that means for
connected clients.

**Acceptance criteria:**
- If clients are connected when I disable the server, Kagami makes the
  consequence explicit before completing the action — naming how many clients
  will lose access — and asks for my confirmation.
- Once disabled, previously connected clients can no longer read the session,
  author edits, or control runs; their outstanding requests fail with a clear
  result rather than hanging, and reconnecting later requires a newly enabled
  server and its new credential.
- Disabling the server never modifies the open experiment and is not a run
  control: a local or cluster run that is active keeps running.
- Disabling never affects the cluster connection; Kagami remains the Orishu
  client whether or not its MCP server is enabled.

### Start Kagami with MCP enabled

As a scientist-researcher, I want to start Kagami with the MCP server already
enabled so that an external client can connect as soon as the app is up,
without me reaching for the UI first.

**Given** Kagami is not yet running
**When** I start it with the MCP flag (for example, `kagami --mcp`)
**Then** the app opens normally and its MCP server is enabled from startup.

**Acceptance criteria:**
- The startup flag and the in-app toggle run the same server under the same
  rules: the UI shows it as enabled, with the same status, endpoint, and
  connected-client count.
- The endpoint and credential of a startup-enabled server are reported at
  startup (for example, on the console or in the log), so a workflow without a
  visible window can still hand them to a client.
- Without the flag, the MCP server stays disabled until I enable it in the
  UI.
- If the server cannot bind at startup, Kagami still opens and reports the
  failure in-app; a failed MCP start never blocks authoring or visualization.

## Authoring and inspection through MCP

### Author an experiment through MCP with UI parity

As a scientist-researcher delegating authoring to an agent, I want every
authoring operation I can perform in the UI to be available through MCP so
that the agent can build and refine the same experiment I could build by hand.

**Given** an experiment is open and the MCP server is enabled
**When** an authenticated client invokes authoring operations
**Then** Kagami applies them with exactly the rules the UI obeys.

**Acceptance criteria:**
- The MCP authoring surface covers at minimum what
  [Modify the experiment](./authoring.md#modify-the-experiment) covers:
  creating, modifying, and deleting world objects and their physical
  properties; the computational domain and its discretization; simulation
  parameters; active fields and physical models; initial conditions; and
  requested observations.
- Every MCP edit follows the authoritative command path and inherits its
  guarantees — validation, atomic commit, revisioning, command identity and
  actor provenance, dirty state, and undo — as specified in
  [Modify an experiment through MCP](./authoring.md#modify-an-experiment-through-mcp).
- Document lifecycle is equally expressible: creating a new experiment,
  opening a saved document, and saving, under the same rules as
  [Experiment documents](./authoring.md#experiment-documents). Where the UI
  would ask me for a decision (for example: unsaved changes before replacing
  the open experiment), the MCP caller must supply that decision explicitly in
  the request; Kagami never resolves it silently.
- An accepted edit reports its result — the resulting revision and the updated
  experiment content — so a client never has to guess what the UI now shows.
- The MCP surface exposes domain authoring operations, never mutable document
  storage or Kagami's internal representation.

### Inspect the experiment and run state through MCP

As a scientist-researcher, I want my agent to read the current experiment and
run state through MCP so that its decisions are based on the same facts the UI
shows me.

**Given** the MCP server is enabled and a session is open
**When** an authenticated client reads experiment or run state
**Then** Kagami returns the same state the UI presents, with the same meaning.

**Acceptance criteria:**
- Reads cover the authored experiment at its current revision: the world —
  every object with its stable identifier, physical properties, pose, and
  velocity — the computational domain and discretization, simulation
  parameters, active fields and models, initial conditions, and requested
  observations.
- Reads cover run state: whether a local or cluster run is active, its state
  (for example: running, paused, stopped, or error), and its simulation time.
- Observation reads carry the same provenance as in the UI — experiment or
  workload identity, simulation time, model, precision, and validity — and MCP
  never recomputes or restates values in a weaker form.
- A client and a user looking at the same session see one state: an MCP read
  reflects edits and run progress made from the UI, and UI displays reflect
  edits made through MCP.
- Reads are read-only: they never modify the experiment, a run, or cluster
  state, and never mark the document modified.

### Discover authoring capabilities through MCP

As an external client acting on a scientist-researcher's behalf, I want to
discover what this Kagami session can author — object and component
vocabulary, available fields and physical models, parameter names, ranges, and
units — so that I can construct valid commands instead of guessing from
hard-coded knowledge.

**Given** the MCP server is enabled
**When** a client asks what the session supports
**Then** Kagami reports its authoring vocabulary and limits.

**Acceptance criteria:**
- Discovery reports the authoring vocabulary: object and component schemas,
  current object-catalog entries and their availability, available plugins and
  their availability, available fields and physical models, and the names,
  units, and valid ranges of simulation parameters.
- Discovery reflects the actual capabilities of the current session, not a
  generic catalog that may disagree with it.
- The MCP surface is self-describing: each operation exposes its name,
  purpose, and input schema through MCP's own discovery mechanism, so a client
  can learn the surface without separate out-of-band documentation.
- Discovery is read-only and never changes session state.
- Rejected commands refer to these capabilities where possible: a validation
  failure names the constraint that was violated, so a client can correct
  itself.

### Manage and instantiate catalog templates through MCP

As an external client acting on a scientist-researcher's behalf, I want to
inspect, edit, and instantiate Kagami's object templates so that delegated
authoring has the same reusable vocabulary as the UI.

**Given** the MCP server is enabled
**When** a client lists, reads, creates, updates, deletes, validates, reloads,
or instantiates a catalog template
**Then** Kagami routes the operation through the same catalog or document
authority used by the UI.

**Acceptance criteria:**
- MCP exposes bounded discovery and CRUD operations with catalog revision,
  source fingerprint, availability, and structured diagnostics.
- Catalog CRUD and reload use the catalog authority and do not dirty the open
  experiment. Stale guarded writes are rejected instead of overwriting a UI or
  filesystem edit.
- Instantiation uses the document authority, including command identity, actor
  provenance, optional base experiment revision, parameter binding,
  expression/dimension validation, atomic commit, and undo.
- The same catalog or instantiation command submitted through UI and MCP paths
  produces the same authoritative decision and resulting projection.
- Removing or changing a template through MCP cannot mutate existing
  experiment objects. Applying a newer template is a separate explicit
  document command.

### Manage plugins through MCP

As an external client acting on a scientist-researcher's behalf, I want to add
and select plugins through MCP so that delegated authoring can use the same
physics vocabulary as the UI.

**Given** the MCP server is enabled
**When** a client adds a plugin, lists available plugins, or selects a plugin
for the experiment
**Then** Kagami routes the operation through the same validation and document
authority the UI uses.

**Acceptance criteria:**
- MCP exposes the same plugin availability, validation diagnostics, and
  physics choices as the UI, including the built-in plugins.
- Adding a plugin through MCP follows the same bounded validation of the
  plugin manifest and kernel against the workload contract and never mutates
  an open experiment.
- Selecting a plugin for the experiment is a normal document command with the
  same revision, undo, and parity guarantees as the UI path.
- The same plugin command submitted through UI and MCP paths produces the same
  authoritative decision.

## Run control through MCP

### Control runs through MCP

As a scientist-researcher delegating execution to an agent, I want to start,
pause, step, and stop runs through MCP so that the agent can verify an
experiment the same way I would from the UI.

**Given** the MCP server is enabled and a valid experiment is open
**When** an authenticated client issues run controls
**Then** the runs respond exactly as they would to the UI's controls.

**Acceptance criteria:**
- Local run controls through MCP have the semantics of
  [Run an experiment locally](./authoring.md#run-an-experiment-locally): the
  same fixed-step advance, the same play/pause/step behavior, and the same
  observation semantics; a run started by a client is the same run the UI
  shows, not a parallel one.
- Cluster submission and control through MCP follow
  [Submit an experiment to a cluster](./authoring.md#submit-an-experiment-to-a-cluster)
  and
  [Control and observe a cluster run](./authoring.md#control-and-observe-a-cluster-run):
  the same completeness checks, the same explicit confirmation to replace an
  active workload (supplied by the caller in the request), the same privilege
  requirements, and the same cluster session the UI uses — MCP never opens a
  second connection path around Kagami's Orishu client.
- Run control results report the run authority's decision — accepted, or
  rejected with a reason — not merely successful delivery of the request.
- Run control through MCP never modifies the experiment: editing and execution
  remain separate authorities, and observing a run never dirties the document.
- Reading run state and observations follows
  [Inspect the experiment and run state through MCP](#inspect-the-experiment-and-run-state-through-mcp);
  observing is read-only.

## Follow-ups

These are deliberately deferred from this story set:

- Non-local exposure: letting clients connect from other machines (the
  "mobile app" case). The stories above keep MCP on local transports;
  widening that needs its own threat model and authentication decision and is
  not a shortcut to the headless multi-user document service.
- Persisting the MCP credential (for example, in a keychain) across app
  sessions; today each enable generates a fresh, unpersisted credential.
- A separate, non-MCP HTTP/REST surface. Field CAD's original "REST API"
  requirement was resolved as MCP over HTTP; a bespoke REST API would need its
  own parity and authority review before it could be adopted.
