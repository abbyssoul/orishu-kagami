# User stories: Kagami

`kagami` is the native experiment authoring and visualization client for `orishu`. It lets a scientist-researcher define, run, and inspect a simulation without operating the cluster directly.

Kagami is an authenticated `orishu` client, not a second runtime. Editable experiment intent (Kagami's authoring state) stays separate from the immutable workload that `orishu` accepts and advances; see [ADR 0001](../../adr/0001-product-oriented-monorepo.md).

Stories in this directory are written from the perspective of a Kagami user authoring and visualizing experiments. They are not written from the perspective of a cluster user or administrator, which are captured in [docs/user-stories/orishu](../orishu/README.md).

The initial collaboration model is file-based for drafts and service-based for
runs: people exchange experiment documents explicitly, while any number of
Kagami clients may independently observe the same Orishu run. Live multi-writer
authoring is deferred, but the document authority remains suitable for later
hosting in a headless collaboration service; see
[ADR 0012](../../adr/0012-start-with-file-sharing-and-preserve-collaborative-authoring.md).

## Personas

### Scientist-researcher

- **Moniker:** scientist-researcher
- **Goals and motivations:** Define scientific experiments visually or declaratively, run them against an `orishu` cluster, and observe and inspect results without hand-writing protocol calls or workload manifests. Delegate parts of authoring and run control to an agent when that is faster, while staying able to take over at the keyboard at any time.
- **Pain points and frustrations:** Authoring a workload by hand is error-prone and disconnected from the running simulation; it is hard to tell what an experiment will do before submitting it, or to relate observed output back to the experiment that produced it. Programmatic interfaces that fork validation or state from the UI make delegated work unsafe to trust.
- **Key tasks/usage scenarios:** Create and edit experiments, choose the plugin whose phenomenon the experiment models (built-in or custom), manage reusable object templates, instantiate catalog objects, configure initial conditions and domain setup, submit an experiment as a workload, visualize live or checkpointed simulation state, review results, and enable or revoke external (MCP) access to the running session.

### Automation client (delegated)

- **Moniker:** automation client
- **Goals and motivations:** Command a running Kagami session on a scientist-researcher's behalf — author experiments, control runs, and read state — with exactly the same guarantees the interactive user gets, so delegated work is safe to trust.
- **Pain points and frustrations:** Programmatic surfaces that fork validation or state from the UI, that fail silently, or that cannot discover what a session supports force agents to guess and make their effects unverifiable.
- **Key tasks/usage scenarios:** Connect to Kagami's MCP server with a user-granted credential, discover and manage catalog templates, instantiate objects, propose experiment edits, start and stop runs, and inspect experiment and run state.

An automation client never acts on its own authority: a scientist-researcher enables the MCP server and grants its credential, and can disable the server at any time.

### Collaborator or observer

- **Moniker:** collaborator
- **Goals and motivations:** Receive an experiment draft from another
  researcher, inspect the same live run, or replay persisted output at an
  independently chosen simulation time and speed.
- **Pain points and frustrations:** Being forced to share another person's
  camera or playback position, requiring edit privileges merely to observe, or
  being unable to relate a shared run to the exact submitted experiment.
- **Key tasks/usage scenarios:** Open a shared experiment file, receive a run
  reference, follow live head, pause or seek locally, and inspect historical
  observations without changing the simulation or another observer's view.

### Plugin author (advanced user)

- **Moniker:** plugin author
- **Goals and motivations:** Extend the physical models Kagami can simulate by
  authoring plugins — a manifest that names the plugin and describes the
  physical phenomenon it models (including the variables it exports), together
  with the kernel code that simulates that phenomenon — using their preferred
  coding tools and IDEs, then adding the finished plugin to Kagami so
  experiments can use it. Kagami ships with a pre-defined set of plugins (for
  example electrodynamics and gravity); the plugin author supplies the ones
  beyond that set.
- **Pain points and frustrations:** Writing kernels inside Kagami would be
  worse than dedicated coding tools; unclear plugin contracts (manifest,
  phenomenon definition, exported variables, kernel capabilities); plugins
  that behave differently on the cluster than they do locally.
- **Key tasks/usage scenarios:** Build a plugin (manifest plus kernel as a
  workload component) against the documented contract, add it to Kagami,
  validate it, share it, author experiments that use it, and submit or export
  experiments whose workload closure includes the plugin's kernel.

## Glossary

- **Scientist-researcher:** The person who uses Kagami to author experiments and visualize `orishu` simulation output.
- **Experiment:** Kagami's editable representation of a simulation setup, prior to being submitted to `orishu` as a workload: the world, computational domain and discretization, simulation parameters, active physics and field choices, initial conditions, and requested observations. This is the entity Field CAD called the "scene".
- **World:** The objects and physical properties authored as part of an experiment; what the viewport displays and what edits act on.
- **Experiment document:** A versioned file persisting an experiment's editable intent. It carries intent only, never computed states or run history.
- **Document authority:** The logical owner that orders proposed authoring commands and accepts or rejects experiment revisions. It initially runs in each Kagami process and may later be hosted headlessly.
- **Object catalog:** Kagami's client-owned collection of versioned object-template files. Editing it does not edit the open experiment.
- **Object template:** A reusable, generic composition of components, parameters, and authored properties. Instantiation creates a self-contained experiment object with source provenance, not a live catalog link.
- **Plugin:** The user-facing unit of Kagami extensibility: a manifest that
  names the plugin and describes the physical phenomenon it models — including
  the variables it exports — together with the kernel code that simulates that
  phenomenon. Kagami ships with built-in plugins (for example electrodynamics
  and gravity); custom plugins are authored outside Kagami and added to it as
  content-addressed workload components.
- **Kernel:** The executable code a plugin registers to simulate its
  phenomenon: a workload component that advances simulation state through the
  versioned lifecycle contract.
- **Workload component:** The content-addressed WebAssembly Component carrying
  a plugin's kernel; see the [orishu glossary](../orishu/README.md#glossary).
- **Run:** One execution of an experiment: an ordered stream of computed states at committed simulation boundaries. A run is local (in-process preview) or a cluster run (submitted to `orishu` as an immutable workload).
- **Run reference:** The information needed to identify and observe one accepted cluster run, including cluster, workload, and workload-epoch identity. It does not carry camera or playback state.
- **Local run:** An in-process run of an experiment inside Kagami for preview and verification, publishing the same observation semantics as a cluster run.
- **Kagami app:** The Kagami desktop application through which experiments are authored, submitted, and visualized.
- **MCP server:** Kagami's embedded server through which authenticated external clients command the running session. It is off by default, enabled by a startup flag or in the UI, requires a fresh credential, and binds only local transports.
- **External client (MCP client):** A program — an AI agent, a script, or another front-end — that connects to Kagami's MCP server and acts with a scientist-researcher's delegated authority.
- **Parity of meaning:** The UI/MCP equivalence guarantee: the same operations, with the same rules and results, expressed through different interfaces. It is not parity of gestures; presentation state is excluded. See [ADR 0006](../../adr/0006-mcp-ui-equivalence.md).

Terms shared with `orishu` (workload, observation, checkpoint, result, simulation boundary) follow the definitions in [docs/user-stories/orishu/README.md](../orishu/README.md#glossary) and [docs/spatiotemporal-foundation.md](../../spatiotemporal-foundation.md).

## Reading order

1. [authoring.md](./authoring.md) - the two-entity model (experiment and run), experiment documents, editing, local and cluster runs, and reviewing previous runs.
2. [mcp.md](./mcp.md) - external control of the running session: MCP server lifecycle, authoring and inspection parity, and run control through MCP.
