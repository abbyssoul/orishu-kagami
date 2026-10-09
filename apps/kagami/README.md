# Kagami

Kagami is the native Orishu client for authoring experiments, controlling
workloads, and visualizing live or recorded observations.

The current crate reuses the Iced application shell and offscreen-capable GPU
renderer from the standalone prototype. It can select and display an Orishu
endpoint and explicitly inspect retained worker runs. Its current window adapter
uses one-shot committed snapshots with 3D position markers and a numeric table,
not a live observation stream.

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
required defaults are not fabricated. Uncaptured exact attachments offer local
quantity, text and boolean fields, including repeated uses of a component; an
unavailable schema can be inspected after dependency resolution without installing
it. Required values must be entered before final attachment. Existing values can
be edited in the inspector.
Dependency ambiguity leaves a contribution unavailable instead of selecting physics.

Use `--plugin-directory PATH` or `KAGAMI_PLUGIN_DIR` to select the same inventory
used by `kagami plugin --directory PATH`. Otherwise it uses
`$XDG_DATA_HOME/kagami/plugins` (or `$HOME/.local/share/kagami/plugins`).
`--enable-plugin ID` and `--disable-plugin ID` are repeatable process-only overrides;
they do not modify the index or saved experiment. Conflicting/unknown overrides
are refused before the window or MCP listener starts. Startup corruption/IO/budget
errors also fail closed. Discovery currently caps component declarations at 256.
The **Plugins** panel lists/inspects installed releases, installs or updates local
bundles, changes persistent enablement and selects the default release. Opening it,
refreshing or completing a mutation reloads verified availability without restart.
It uses the same revision-checked authority as the CLI; concurrent changes require
explicit refresh/retry. Process-only overrides are shown separately and still win.
A changed revision clears unapplied physics-form choices and prepared candidates,
but never migrates exact document pins, dirties the document or changes a worker
run. Closing the panel does not cancel an in-flight inventory write or release
document leases. **Remove registration…** offers revision-checked removal with
explicit open-reference acknowledgement; it never purges cached bytes. Pending
reference reconciliation blocks removal even with acknowledgement. Failed scans
require **Recheck reference leases** or another document action, not silent retry.
Pack and validate remain CLI operations. Component-only dependency UI, automatic watching and MCP management
remain integration work. Explicit physics setup, captured-field reinitialization and headless workload export
are supported as described below, alongside initial manual window run controls.

Native **Inspect** and CLI list/inspect report the release registration's first
known local input origin. It is historical metadata, never an automatic update
location; deleting/moving that input does not prevent cached inspection or change
release identity. Inventory v1 remains readable and upgrades to v2 only on an
accepted management mutation. CLI JSON now uses `kagami.plugin-command/v2` to
version the added origin fields. See [origin encoding and migration](../../docs/plugin-authoring-tools.md#store-and-reader-safety).

**References** in the Plugins panel reports whether an exact release is retained
by this session's current experiment, undo/redo or accepted request data. It is a
bounded point-in-time query, not a lease or discovery of unopened files. Exhausted
queries report incomplete coverage rather than claiming a release is unused.

The window has two explicit modes (ADR 0022). **Authoring** edits initial
conditions. **Observation/replay** shows one run and exposes no document,
undo or redo controls at all — they are absent rather than disabled, and
`kagami_session::Workspace` refuses any command that would reach the authority
in that mode. Remote run → Observe this exact run now enters this mode with a
committed position markers or a numeric object/force table. The Unix window can prepare/export/submit
the open captured experiment; local preview, field visualization and historical
playback remain implementation work.

The viewport offers perspective and orthographic projection. In Authoring, the
projection and camera pose are saved in the experiment file's separately
versioned, client-owned `defaultView` section: they advance their own view
revision and mark the file modified, but never the experiment revision, undo
history or workload identity. Camera changes made while observing are
ephemeral and dirty nothing.

## Window remote-run workflow

Start with an operator credential for an explicitly enabled scientific worker:

```sh
make run-kagami ARGS="--host /path/to/worker.sock --operator-token-file /path/to/operator.token"
```

Use `--ca-cert PATH` when additional TLS trust is required. Credentials remain
process-local; they are not stored in an experiment or workload. Startup does not
connect automatically. You can inspect an externally submitted run, or prepare
and submit the open experiment through the same panel.

On Unix, **Prepare captured revision** compiles the open experiment's captured
scientific state and exact selected plugins without initializing fields again.
Enter a workload name; the panel reports the frozen root, source revision,
inventory revision, byte count and artifact count. Saving the experiment first
is unnecessary. Draft edits or replacement invalidate preparation, but moving
the camera does not. A changed/unavailable inventory refuses rather than choosing
another provider. **Export frozen workload…** writes a new portable file and
refuses an existing destination.

To run it, enter the **exact target formation ID** and choose **Submit frozen
workload**. The worker must already be locked and scientifically enabled; Kagami
does not provision or lock it. A lost reply retains the original operation and
frozen bytes for **Reconcile original load** or **Resubmit identical frozen
workload**. Do not equate Pending or missing history with refusal. New preparation
is blocked until final submission history is explicitly cleared. Clearing history
is local and does not stop or unload the worker. Load recovery is currently
memory-only: export a copy before submission if durable retry material is needed,
and record the displayed identity before closing with an unresolved operation.

After an Accepted receipt, **Inspect accepted run** checks that exact execution,
then **Observe this exact run** enters Observation. The panel distinguishes its
captured source revision from an unrelated external run and warns that the open
draft may since have changed. An accepted historical receipt alone cannot restore
a run after worker restart.

Open **Remote run**, choose **Inspect retained run**, then **Observe this exact
run**. The window shows the formation, workload, epoch, committed boundary and SI
time. Authoring controls disappear; numeric positions, velocities and forces come
only from the validated observation. The table displays at most 100 objects and
labels the complete count and force-evaluation boundary. **3D positions** instead
shows fixed-size gold (Dynamics) and blue (static/kinematic) markers—not physical
radii or meshes. Orbit, pan, dolly, projection and scene scale are observer-local.
Up to 65,536 positions are shown, with separate counts for capacity and scale-range
omissions; the camera may additionally clip markers. Scale conversion runs in the
background from cached SI positions and hides old-scale geometry until ready.
Neither camera nor scale changes modify the experiment or run.

**Field inspection / point sampling** reads a configured field instance at the
displayed boundary. Enter its exact instance ID (available in the authored setup
or workload), then **Inspect this field** and choose declared channels. Enter
points as `x,y,z; x,y,z` in metres and choose **Sample selected channels**.
No installed authoring plugin is required. The panel shows exact source/state,
kernel, precision, channel shape/SI dimensions/frame, quality flags and explicit
singular/outside-domain/undefined values. Invalid samples never appear as zeros.
Up to 4096 points and 16 channels are requested, bounded by the selected kernel's
policy and 64 KiB of point text. The table displays the first 256 cells and at
most 16 values per cell, clearly separate from complete request coverage.
This is a manual read, not a saved instrument or subscription.
Descriptors do not pin later queries: if another controller advances the run,
sampling refuses the stale boundary. Refresh and inspect again explicitly.
Query settings/results are client-local and never dirty the experiment; changing
inputs, refreshing, stepping or detaching invalidates the corresponding old data.

For a vector-3 channel, check its frame, axes and conventions, enter a finite
positive **arrow length in metres**, and choose **Map this channel to world X/Y/Z
and draw directions**. This is an explicit interpretation, not an automatic
coordinate transform: do not map non-Cartesian/unknown frames directly. Green
arrows show normalized direction only, **not magnitude or quality**; numeric values
and quality remain in the report. All queried points participate, not merely the
truncated numeric table. Invalid/zero vectors and scale/precision omissions are
counted separately. Camera motion reuses cached geometry; scale changes reproject
locally without another worker request. Arrows share depth with object markers;
subpixel/view-axis directions and arrows crossing near/far clip planes are not
drawn. **Hide directions** retains the numeric report. Flow lines, magnitude
mapping and persistent field instruments remain follow-up work.

**Refresh committed values** explicitly obtains a later snapshot. **Step once**
and **Finish run** use the confirmed run/boundary, affect every observer and never
retry automatically. Finish is terminal, not pause. Refresh after a successful
command before issuing another; a receipt is historical evidence, not a frame.
After a lost reply, reconcile the retained operation or explicitly resubmit its
identical original intent. No new command is allowed while that intent is unresolved.
Client recovery intent is currently in-memory only: record the displayed run,
operation, action and boundary before closing if reconciliation is still pending.

**Return to open authoring document** detaches without finishing the worker run
and restores the draft and its authoring view without computed-state adoption. Closing the panel during
an unattached read cancels adoption, but retains the background slot until the
request exits. Remaining work is tracked in
[K-RUN](../../docs/tasks/implement-kagami-run-workflow.md).

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
[Source v2](../../docs/plugin-source-v2.md) accepts explicit local contract and
artifact aliases, lowered in dependency order during validation/packing; v1 exact
sources remain supported. This hashes externally built code, never compiles or
executes it, and equivalent sources produce identical installed bundles.
`PluginStore::prepare_selection` retains the exact selected artifacts and release
leases; `scientific::LocalInitializer` can produce opaque field/history candidates
through the worker-compatible sandbox. Their captures can enter the document
authority as one validated/undoable edit. The existing file store saves/reopens
scientific setup in the [v4 container](../../docs/experiment-container-v4.md) without
initializing or requiring executable installation. Drafts carrying standalone
component provider intent instead use [v5](../../docs/experiment-container-v5.md);
the document core, catalog-v3 materialization and the Unix existing-component
choice form support it. See [component provider choices](../../docs/component-provider-choices.md).
Select **Scientific setup / fields**
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
changing the document. **Apply only this field's parameters**, with its separate
reset confirmation, reinitializes only that field and preserves other fields and
integrator history. Domain, timestep and other model parameters must be unchanged;
unrelated pending edits are refused, not silently dropped. A changed document or
inventory requires copying settings again. The whole-setup **Apply physics /
initialize** still resets **all** initial fields/history. **Start new physics proposal**
discards local settings/pins without changing the experiment. Missing exact
models refuse a copy; no alternative is chosen.

After configuration, attaching/detaching Dynamics, changing its mass or removing a
dynamic object prepares updated initial integrator history in the background.
Objects and history become one undoable revision; fields retain their captured
state. Failure, cancellation or an intervening edit leaves the old proposal
unaccepted. Ordinary edits such as renaming do not execute kernels. The effect API
also supports compound creation and initial pose/velocity edits; dedicated gesture
and MCP adapters remain follow-up work. Scene proposals cap commands/components/
properties at 4096 total entries and property text at 1 MiB, in addition to the
document's per-input and collection bounds. These are not a process RSS guarantee.

Discovery separately caps model choices at 256 and parameter metadata at 8 MiB.
The form retains at most 4096 explicit inputs and 1 MiB of input text, with a
4096-byte per-input ceiling in addition to schema constraints. It lists enabled contributions,
not a guarantee that their dependencies/configuration can be satisfied.
The **Scientific dependencies → Check dependencies** control resolves the current
model and scene-component roots without running kernels or changing the document.
Ambiguous slots show exact provider identities, with 32 candidates per page and
explicit diagnostic truncation. Choose a provider, check again for remaining or
transitive requirements, then confirm **Apply**. The choice enters the captured
experiment and exported workload only after normal initialization/validation.
Changing numeric form inputs preserves explicit choices but invalidates old reports;
changing selected models clears local bindings. **Clear local provider choices**
does not edit accepted state. Captured bindings remain immutable unless you start
a new proposal. Document/form/inventory changes and old-page clicks cannot reuse
stale results. **Browse installed alternatives** is also available for resolved
bindings and reported external requirements. It includes compatible enabled
non-default releases; choosing one never changes inventory defaults. Browse pages
have the same stale-click protection. Local-only dependency slots cannot be rebound.
Reads use one background slot, retained until actual completion;
failure requires explicit retry. No default provider is guessed.

The **Component provider choices** section supports **Load current component
choices**, bounded checks and candidate browsing, then **Save component choices
(undoable)** for components already in the document before scientific capture.
No field or integrator selection is required. Saving independently revalidates
the exact closure under inventory/document guards and retains release leases;
offline files and undo preserve the choices. Unused local bindings require an
explicit discard action. Saved component bindings constrain later physics
proposals. The object inspector now offers **Resolve and add** for enabled
unavailable components. Adding a new exact root, including the first available
component in an unlocked uncaptured document, also stages
a proposal; **Attach component and save choices** accepts defaults, schema and
lock atomically without rebinding existing edges. Cancel/stale/invalid proposals
leave both capabilities and intent unchanged. Native removal explicitly prunes
the lock and regenerates required history in one edit. **Load property fields**
inspects an unavailable selected schema without changing the document. Local
quantity/text/boolean entry preserves expressions and false/empty/unset distinctions;
every uncaptured exact attachment offers it. **Choose replacement…** stages a
different exact component with new defaults/values; final acceptance replaces the
component and scoped lock together, preserving shared providers. Values are never
implicitly copied or converted. Captured provider replacement has a guarded
full-reset backend and native two-stage workflow. Stage new component values or
binding-only choices, select complete physics, check dependencies and explicitly
confirm resetting all fields/history. Consent names both proposal generations;
changed inputs, cancellation and stale context cannot accept partial edits.
See [component workflows](../../docs/component-provider-choices.md).

Captured **Add component** now uses the same staged values and choices, including
repeated roots. Confirm history regeneration for those exact inputs, then choose
**Add component and regenerate history**. Changing values/choices invalidates
consent; cancellation or withdrawal prevents acceptance even after handoff.
`ScientificEffects::extend_components` independently verifies new component
evidence, retains existing kernels/providers and field bytes, regenerates history
and accepts the whole change with schemas atomically. Success clears the proposal;
failure retains local values but needs fresh consent and explicit retry.
The backend now also exposes `compose_scene` for bounded object/component/new-
definition batches and `instantiate_template` for one fingerprint-pinned catalog
snapshot. They preserve fields/provider choices and accept copied definitions,
history and schemas atomically. **Catalog → Load directory…** now opens bounded
template pages through the shared catalog authority. Select an exact template,
enter name/SI position/velocity and parameter expressions, check/choose dependency
providers and explicitly create. Captured creation additionally requires current
history consent. Unavailable entries are revalidated against selected schemas;
template/document bindings cannot be replaced. Reload refreshes the read projection
without changing existing objects. See the [native catalog workflow](../../docs/component-provider-choices.md#native-catalog-creation).
Arbitrary compound/migration planning, catalog-file editing and headless adapters
remain pending; this is single-template instantiation, not a catalog editor.

Scientific MCP parity, plugin-management
UI/MCP, other guarded effects and complete run workflow remain in progress;
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
emitter/dynamic-membership support remains open. Unix window export uses the same compiler.

## Headless workload submission

Unix `kagami workload` submits a previously exported complete bundle through the
shared scientific client, without opening a window, MCP server or plugin inventory.
Enable the worker's experimental scientific API and explicitly lock a standalone
formation with operator tooling first. Obtain its immutable formation ID and the
root digest printed by export; substitute those values below:

```sh
kagami --host /path/to/worker.sock workload \
  --operator-token-file /path/to/operator.token --json \
  submit experiment.orishu --formation-id formation-a \
  --operation-id upload-001 --workload-id 'sha256:<digest-from-export>'

kagami --host /path/to/worker.sock workload \
  --operator-token-file /path/to/operator.token --json \
  receipt --formation-id formation-a \
  --operation-id upload-001 --workload-id 'sha256:<digest-from-export>'

kagami --host /path/to/worker.sock workload \
  --operator-token-file /path/to/operator.token --json current
```

`--host`/`-H` precedes `workload` and otherwise follows the normal `ORISHU_HOST`/
local-worker default. Put `--operator-token-file` and optional `--ca-cert` before
the operation. `ORISHU_OPERATOR_TOKEN_FILE` may supply the explicit credential-file
path; there is no raw-token argument. TLS/TCP uses the same shared client; `--ca-cert`
adds trust for the configured worker. Secure credential loading is shared with
`orishuctl`: an owned, private, singly linked regular file with exactly the worker's
64 lowercase hexadecimal bytes, no trimming, symlink following or FIFO blocking.
Non-Unix credential/bundle IO remains unsupported.

Submission bounds the regular input file to 128 MiB, verifies the complete portable
closure and expected root locally, then sends the exact request once. The worker
independently admits it. No document rewrite, field initialization, plugin lookup,
membership lock, fresh-ID retry or automatic polling occurs. Error reports retain
the original intent; `submissionOutcomeUnknown: true` means delivery/acceptance may
have happened. Query that same operation at that same worker before deciding what
to do next. A final refused/indeterminate ID cannot be reused as a new execution.

These load/discovery commands emit one `kagami.workload-command/v1` JSON report, compact
with `--json` or indented otherwise. Strings are JSON-escaped, and reports reflecting
operator credential material are rejected instead of printed. A request using
credential material in its identity is refused locally without reporting that
identity. Parse the `outcome` and receipt, not merely transport success:

| Exit | Outcome | Meaning |
| --- | --- | --- |
| 0 | `accepted` / `live` | Historical initial publication / currently usable descriptor, respectively |
| 1 | `error` | Local, transport, protocol or HTTP error; inspect the uncertainty flag |
| 2 | No report | Command-line usage error, not Pending |
| 10 | `pending` | Durable operation still in progress; not acceptance |
| 11 | `refused` | Durable known refusal |
| 12 | `indeterminate` | Unknown historical outcome; no automatic re-execution |
| 13 | `notFound` / `empty` | No exact receipt / no currently usable run, respectively |

A saved Accepted receipt can remain after restart while `current` is Empty. It
does not restore scientific state. These commands load/publish initial state;
manual step/finish/status and exact object/field reads use the commands below.
Pause/resume/unload, streaming and interactive playback remain work. See the
[load HTTP profile](../../docs/protocol-scientific-load-v1.md).

`make test-kagami-workload` builds an actual worker and runs the explicit
cross-application upload/control/discovery/restart test with real Newtonian/Euler Components.
The ordinary `workload_cli` tests exercise malformed local inputs, exact wire bytes,
lost replies, receipt states and credential confidentiality without a GUI.

## Headless manual run controls

Use the exact formation/root/epoch from the accepted descriptor. An operation ID
identifies immutable command intent; preserve the same action and expected
boundary when reconciling it. `current` and `status` do not silently fill in or
refresh a mutation's target. Each `step` computes exactly one workload-defined
fixed timestep. `finish` permanently stops integration at its expected boundary
while retaining results; it is **not pause or unload**.

```sh
kagami --host /path/to/worker.sock workload \
  --operator-token-file /path/to/operator.token --json step \
  --formation-id formation-a --workload-id 'sha256:<digest-from-export>' \
  --workload-epoch 1 --operation-id step-001 --expected-boundary 0

kagami --host /path/to/worker.sock workload \
  --operator-token-file /path/to/operator.token --json command-receipt \
  --formation-id formation-a --workload-id 'sha256:<digest-from-export>' \
  --workload-epoch 1 --operation-id step-001 --expected-boundary 0 --action step

kagami --host /path/to/worker.sock workload \
  --operator-token-file /path/to/operator.token --json status \
  --formation-id formation-a --workload-id 'sha256:<digest-from-export>' \
  --workload-epoch 1

kagami --host /path/to/worker.sock workload \
  --operator-token-file /path/to/operator.token --json finish \
  --formation-id formation-a --workload-id 'sha256:<digest-from-export>' \
  --workload-epoch 1 --operation-id finish-001 --expected-boundary 1
```

Epoch must be nonzero. Mutation/receipt commands require an explicit boundary and
operation ID; receipt lookup also requires `--action step|finish` and never
dispatches that action. Invalid next boundaries refuse locally. All controls use
the same shared client, secure credential loading and escaped/redacted JSON output
as submission. They do not open a plugin inventory, modify an experiment, change
membership, initialize fields, poll, retry or adopt observed state as authoring.

These four commands emit `kagami.run-command/v1`, keeping existing load-report
schemas unchanged. `request` is the exact versioned command or status query;
`receipt` is historical, whereas top-level `status` is a separately queried live
projection. Applied receipts never masquerade as current status. Exit codes are
the table above, with `applied` (exit 0) replacing `accepted` for known command
application. `status` yields `live`/0 whenever exact metadata is available, even
if its `phase` is `finished`; inspect that phase rather than interpreting `live`
as actively integrating. Unavailable exact status yields `empty`/13. Unknown
transport/protocol outcomes keep the original intent and flag possible delivery;
do not mint a new operation ID or refresh a boundary as an automatic retry.

After restart, command receipt lookup can still return Applied while exact status
is Empty. Neither history nor a stale-boundary refusal proves restored scientific
state. The worker currently retains 256 command records without eviction; sustained
control/history policy remains work. See the [command HTTP contract](../../docs/protocol-scientific-command-v1.md).

## Headless committed object observations

Read numeric objects at one exact committed boundary without advancing the run:

```sh
kagami --host /path/to/worker.sock workload \
  --operator-token-file /private/operator.token --json objects \
  --formation-id formation-a --workload-id 'sha256:<root>' \
  --workload-epoch 1 --boundary 1
```

Supply the accepted run tuple and desired boundary explicitly (for example from
`workload status`). No operation ID is needed for a read. A stale boundary is an
error, not permission to refresh or retry silently. The command uses the shared
authenticated [object-read client](../../docs/protocol-object-observation-v1.md)
and does not load plugins, initialize kernels, edit the document or enter the
window's observation mode.

The new `kagami.object-observation/v1` JSON report retains `request` and, on success,
an `observation` with:

- `digest` of the verified portable frame and its exact committed `source`;
- `coverage: "complete-numeric-objects"`, excluding fields and other authored components;
- `objects`: run-local `id`, `kinematics.position_metres`,
  `kinematics.velocity_metres_per_second`, and nullable `inertial_mass_kilograms`;
- `forces`: rows with `id` and `newtons`, or null at boundary zero;
- `forceEvaluationBoundary`: N-1 for forces used to produce boundary N, or null
  at boundary zero. These are not recomputed forces at the displayed positions.

Values use finite binary64 interchange and Cartesian SI units. This does not
claim binary64 kernel arithmetic; exact compute provenance comes from the workload.
Static objects remain present. An empty object array is a successful observation,
not an unavailable run. No raw binary payload, command receipt or mutable status
is embedded in this report. `observed` exits 0, unavailable exact run `empty` exits
13, and stale/busy/missing API/transport/invalid data exits 1. All read failures
keep `submissionOutcomeUnknown: false`; there was no mutation to reconcile.
Credential reflection is rejected before reporting, with the safe original intent
retained. Existing load/control report versions and field shapes are unchanged.

Input is capped at the worker profile's 16 MiB. JSON can be larger; this is a
bounded one-shot inspection path, not a frame-rate streaming API. Serialization
iterates borrowed numeric records without constructing per-object collections.
The CLI still materializes JSON for its credential check/output and revalidates
the immutable view per serialization; it is not an allocation-free renderer path.
Physical object shapes, field sensors, baselines/resume and playback remain
separate work. `make test-kagami-workload` exercises this command against real
Newtonian/Euler Components as well as load, controls and restart history.

## Headless field inspection and point sampling

Discover one exact field instance and its available typed channel slots:

```sh
kagami --host /private/worker.sock workload \
  --operator-token-file /private/operator.token --json field \
  --formation-id formation-a --workload-id 'sha256:<root>' \
  --workload-epoch 1 --boundary 1 --field newtonian
```

Use the desired `field.context.observables[].slot` in an explicit point query:

```sh
kagami --host /private/worker.sock workload \
  --operator-token-file /private/operator.token --json sample \
  --formation-id formation-a --workload-id 'sha256:<root>' \
  --workload-epoch 1 --boundary 1 --field newtonian \
  --request-id 42 --channel acceleration \
  --point '1,1,1' --point '0,0,0'
```

`field` names a configured workload instance, not a plugin/family to resolve.
The example uses the Newtonian reference model's `acceleration` slot; use the
selected descriptor's slots for other models.
Coordinates are finite Cartesian SI metres in the selected domain frame. Repeat
`--point x,y,z` for 1–4096 positions; IDs are zero-based argument order. Repeat
`--channel` for 1–16 distinct selected-model binding slots, preserving order.
The selected context can impose tighter counts/output limits. Request IDs are
observer-scoped, not scientific command operation IDs.

Both commands emit `kagami.field-observation/v1` JSON with the safe original read
intent. `field` returns the shared descriptor: committed source, state identity,
selected model/context, compute precision, typed channels/dimensions and bounds;
never private field bytes. `sample` first describes the explicitly named boundary,
then binds the query to that exact context/state using the shared sampling ABI.
It never refreshes a stale boundary, retries or advances the simulation.

Successful sampling adds `samples` with request/response digests, exact scientific
metadata, `coverage: "complete-request"` and `readings`. Each reading contains its
point ID/position and cells in requested channel order. Valid cells have `valid:
true`, finite row-major SI `values` and independent `qualityFlags` (direct=1,
interpolation=2, reconstruction=4). Invalid cells have `valid: false` and an explicit
`reason`, with **no** numeric values or quality flags. Numeric shape, units and
coordinate conventions come from the exact channel schema, not vector length.

An unknown slot fails with `channel_unavailable` before sending a sample request;
the descriptor remains available for choosing a declared slot. This convenience
CLI selects channels from the active model. It does not implement persisted probe
requests for unavailable scientific contracts; the lower-level shared sampling
API retains those explicit ChannelUnavailable semantics.

Success is `observed`/exit 0 even when individual cells are invalid. A missing
retained run is `empty`/13. Malformed, stale, busy, failed guest, transport or
protocol results are errors/1 and never claim uncertain mutation. A descriptor
obtained before a later sample failure may remain in the report; it is not live
status or proof that sampling succeeded. Credential reflection clears all remote
descriptor/sample data while retaining safe original intent. Syntax errors are
Clap exit 2.

The [field wire profile](../../docs/protocol-field-observation-v1.md) bounds
descriptor and sample packets; JSON can be larger. Cells serialize from borrowed
validated buffers without allocating a vector for each cell, but the CLI still
materializes JSON for output/redaction and repeats bounded packet validation.
This is cold one-shot inspection, not a renderer loop or a sensor subscription.
No document/inventory is opened, no fields initialized, and no window observation
mode entered. Instrument geometry, attachments, retained history, UI/MCP adoption
and local/proxy window parity remain separate work. `make test-kagami-workload`
exercises these commands through real Newtonian/Euler execution and restart.
