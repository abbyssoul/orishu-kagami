# Kagami

Kagami is the native Orishu client for authoring experiments, controlling
workloads, and visualizing live or recorded observations.

The current crate reuses the Iced application shell and offscreen-capable GPU
renderer from the standalone prototype. It can select and display an Orishu
endpoint, but it does not connect or stream observations yet.

The scene tree and inspector are no longer demo state: they render the read
projection of a real [`kagami-session`](../../crates/kagami-session)
`DocumentAuthority`, and every edit is a submitted command envelope — the same
one an MCP client would send (ADR 0006). The window owns presentation state and
nothing else, so a refused edit is reported and leaves the view showing the last
accepted revision. File → New, Open, Save and Save As read and write the
versioned `kagami.experiment` format through the durable write protocol.

On Unix, startup loads verified component schemas from the local plugin inventory.
The inspector offers their exact release-qualified types alongside the historical
demo schemas (which are not substitutes for pinned contributions). Adding a plugin
component authors its declared defaults through the document authority; missing
required defaults are not fabricated. Other values can be edited in the inspector.
Dependency ambiguity leaves a contribution unavailable instead of selecting physics.

Use `--plugin-directory PATH` or `KAGAMI_PLUGIN_DIR` to select the same inventory
used by `kagami plugin --directory PATH`. Otherwise it uses
`$XDG_DATA_HOME/kagami/plugins` (or `$HOME/.local/share/kagami/plugins`).
`--enable-plugin ID` and `--disable-plugin ID` are repeatable process-only overrides;
they do not modify the index or saved experiment. Conflicting/unknown overrides
are refused before the window or MCP listener starts. Startup corruption/IO/budget
errors also fail closed. Discovery currently caps component declarations at 256.
Dependency selection dialogs, live inventory reload and open-document leases remain
integration work. Explicit physics setup, captured-field reinitialization and headless workload export
are supported as described below; run control is not enabled yet.

The window has two explicit modes (ADR 0022). **Authoring** edits initial
conditions. **Observation/replay** shows one run and exposes no document,
undo or redo controls at all — they are absent rather than disabled, and
`kagami_session::Workspace` refuses any command that would reach the authority
in that mode. Nothing enters Observation/replay yet: submitting a run belongs
to K-RUN and previewing one to K-PREVIEW, so the toolbar still reports
`Run: unavailable`.

The viewport offers perspective and orthographic projection. In Authoring, the
projection and camera pose are saved in the experiment file's separately
versioned, client-owned `defaultView` section: they advance their own view
revision and mark the file modified, but never the experiment revision, undo
history or workload identity. Camera changes made while observing are
ephemeral and dirty nothing.

## MCP server

Kagami embeds an MCP server so authenticated external clients — AI agents,
scripts, alternative front-ends — command the **same live session** the window
uses (ADR 0006). It is a transport over Kagami's authorities, not a second
model: presentation state stays client-local to the window.

The server is **off by default**: with it disabled no port is bound and no
request can reach the session. Enable it in the running app from **Settings →
MCP server**, or start with `--mcp`:

```sh
make run-kagami ARGS="--mcp"
```

- **Endpoint.** Loopback only, `http://127.0.0.1:8642/mcp`. A non-loopback bind
  is refused outright; exposing the server beyond this machine is a separate
  decision. If the port cannot be bound, Kagami reports the reason, leaves MCP
  disabled, and stays fully usable.
- **Token.** Every request must carry `Authorization: Bearer <token>`, even on
  loopback. The token is a fresh UUID generated per enable and **never
  persisted**; re-enabling mints a new one and old tokens stop working. In the
  window it is shown masked with a copy action; started with `--mcp`, the
  endpoint and token are printed to the console so a windowless workflow can
  hand them to a client.
- **Connections.** While enabled, the panel shows the connected-client count
  (including zero). A client that vanished without closing its session may stay
  counted until the server notices. Disabling while clients are connected names
  how many lose access and asks to confirm.
- **Tools.** One read-only tool, `kagami_status`, reports the app identity and
  the open experiment's status. Authoring and run-control tools arrive with the
  authorities they command; they are not documented here until they exist.

Enabling, disabling, and serving never modify the open experiment, a run, or the
cluster connection.

From the repository root:

```sh
make run-kagami
make run-kagami ARGS="--host cluster.example.com:6680"
make smoke-kagami
```

Use `--exit-after SECONDS` for bounded windowed smoke testing. On systems where
Vulkan presentation misbehaves, try `WGPU_BACKEND=gl` or
`ICED_PRESENT_MODE=no_vsync`.

See the [project architecture](../../docs/architecture.md) and [migration
strategy](../../docs/migration.md).

## Local simulation plugins

`kagami plugin` runs headlessly: validate, pack, inspect, install, update, list,
set-default, enable, disable and remove. `--json` emits structured outcomes;
`--directory` selects an isolated inventory. The initial secure filesystem adapter
is Unix-only and never executes plugin code. See
[source format, commands and limitations](../../docs/plugin-authoring-tools.md).
`PluginStore::prepare_selection` retains the exact selected artifacts and release
leases; `scientific::LocalInitializer` can produce opaque field/history candidates
through the worker-compatible sandbox. Their captures can enter the document
authority as one validated/undoable edit. The existing file store saves/reopens
scientific setup in the [v4 container](../../docs/experiment-container-v4.md) without
initializing or requiring executable installation. Select **Scientific setup / fields**
in the scene tree: the Unix inspector lists captured fields and offers
**Reinitialize field** in Authoring
mode. This explicitly restores that kernel's natural state, leaves other fields
and integrator history untouched, and creates one ordinary undoable revision.
Undo/redo restores bytes without executing code. The toolbar reports the pending
job; Cancel retains the slot until native work actually exits. Document edits,
replacement, a schema refresh or an intervening observation session discard stale
results. Moving the camera does not. Inventory changes cause refusal rather than
provider substitution; restart to refresh the current startup inventory projection.
The same inspector offers **Configure physics**, including on a new experiment.
Choose exact installed field models and one integrator, domain corners in metres,
continuous or Cartesian-cell discretization, and a fixed timestep in seconds.
Kernel parameters are generated from the exact plugin schema: quantities retain
expressions/units, booleans are explicit values and text obeys UTF-8 byte limits.
**Provide value** overrides a default; unchecking it restores default/omission.
Explicit `false` and empty text are not absence. New proposals use binary64 and
direct sampling; unsupported choices and unresolved dependencies are refused. Confirm
replacing physics/resetting initial states before applying. Fields initialize
independently; integrator history is built from current objects with the exact
selected Dynamics component. Objects are preserved and the entire candidate
becomes one undoable revision only after all initialization and validation succeed.
Legacy unpinned components require explicit migration, not automatic substitution.
**Copy captured settings into form** restores saved expressions, instance IDs,
dependency pins, precision and sampling policy without executing a kernel or
changing the document. Editing those settings and applying still resets **all**
initial fields/history, not only the edited field. **Start new physics proposal**
discards local settings/pins without changing the experiment. Missing exact
models refuse a copy; no alternative is chosen.
Discovery separately caps model choices at 256 and parameter metadata at 8 MiB.
The form retains at most 4096 explicit inputs and 1 MiB of input text, with a
4096-byte per-input ceiling in addition to schema constraints. It lists enabled contributions,
not a guarantee that their dependencies/configuration can be satisfied.
Targeted parameter edits preserving untouched field states, dependency-choice
dialogs, scientific MCP parity, plugin-management
UI/MCP, other guarded effects, window export and run control remain in progress;
package validation is not scientific
admission.

`workload::compile_captured` is a bridge from a captured document revision
and an exactly matching prepared plugin selection to the shared numerical workload
compiler. It does not reinitialize state or mutate the document. Its closure runs
in a fresh runtime with no installed plugins. Its shared scene artifact preserves
complete component values, geometry, expression sources and location-free template
fingerprints, with independent scene-to-packet validation.

`kagami export experiment.kagami --output experiment.orishu --name gravity --json`
now publishes a new portable workload from a saved captured experiment. Use
`--directory` for an isolated inventory and `--expected-inventory-revision` for an
explicit selection guard. Exact code must be installed for export; the resulting
file requires no inventory for runtime admission. No initialization, document
mutation or worker submission occurs. See [format, options, refusal behavior and
current limits](../../docs/workload-bundle-v1.md). The secure command is Unix-only;
emitter/dynamic-membership support, window controls and worker delivery remain open.
