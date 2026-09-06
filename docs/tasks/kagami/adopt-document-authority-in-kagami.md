# Adopt the document authority in the Kagami app

Status: **specified**; gated on [K2](integrate-document-variables.md),
[K4](persist-experiment-documents.md),
[K5](instantiate-catalog-templates.md), and the X-PLUGIN schema-inventory
contract. The [K1/K3 boundary follow-up](harden-document-boundaries.md) this
task also needed has landed  
Work package: **K-DOCUMENT** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0004](../../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0006](../../adr/0006-mcp-ui-equivalence.md),
[ADR 0012](../../adr/0012-start-with-file-sharing-and-preserve-collaborative-authoring.md),
[ADR 0019](../../adr/0019-kagami-experiment-document-model.md)  
Stories: [Modify the experiment](../../user-stories/kagami/authoring.md),
[Create a new experiment](../../user-stories/kagami/authoring.md)

## Outcome

`apps/kagami` stops being a demo. The scene tree and inspector render the
document authority's read projection, every edit is a submitted envelope, and
File → New/Open/Save operate on a real experiment. This is the Milestone 2 exit
criterion: *"Kagami creates, edits, saves, reopens, and deterministically
recompiles a minimal experiment revision through the document authority; the
demo scene is not treated as domain state."*

It also retires the
[migration.md](../../migration.md) row that keeps
`apps/kagami/src/scene_model.rs` alive "until an authoritative experiment model
replaces it".

## Owning boundary

The app is the **imperative shell** (`docs/Coding style.md`): it translates IO
— pointer events, keystrokes, dialogs, MCP requests — into envelopes, executes
returned effects, and feeds outcomes back as messages. It owns presentation
state and nothing else. Removing the app must not remove a rule; every rule
lives in `kagami-document`/`kagami-session`.

## Implementation slices

### 1. Replace the demo state

- Delete `apps/kagami/src/scene_model.rs` and its `Model` fields. Hold a
  `DocumentAuthority` and the latest `ExperimentView` instead.
- Iced's `update` maps a `Message` to an envelope, submits it, and stores the
  outcome. A rejection becomes visible feedback, never a silent no-op — the app
  must not present an edit as authoritative before the revision advances.
- Keep the existing TEA shell (`model.rs`, `message.rs`, `update.rs`,
  `view/`); this is a change of *what the model holds*, not of the
  architecture.

### 2. Separate the two command queues

`TODO.md` records the Doom 3 framing that motivates this: a client produces
commands, and the server is authoritative about whether they applied. There are
two queues, and conflating them is the failure mode.

- **Authoritative** — object, component, variable, setup, undo/redo,
  instantiation, and document-lifecycle commands go to the authority.
- **Client/view-local** — camera, projection, selection, expanded tree nodes,
  search query, active tool, per-object hiding, panel layout. These never enter
  experiment commands. In Authoring mode the supported opening-view subset
  advances K4's separate view revision and dirties the file; transient UI and
  all Observation/replay view edits remain ephemeral (ADR 0022).
- A test asserts that no client-local message path can produce an envelope.

### 3. A schema-driven inspector

- Render an object's components from their registered `ComponentSchema`, and
  offer every registered-but-unattached schema for attachment. The inspector
  contains no knowledge of any specific component and no branch on a component
  name — this is Field CAD ADR 0021's exit criterion, and it is what makes a
  new plugin's component authorable the moment its schema is registered.
- Expression-capable properties are edited as *text*: the field shows the
  authored source, and the resolved value is displayed beside it as derived
  output. Editing shows the resolution or the diagnostic (ADR 0005).

### 4. Interactive edits

- A viewport drag or a held inspector control opens a gesture on the authority
  and commits it on release, so the whole gesture is one undo step (ADR 0019).
- Only *held* controls count: a checkbox or a menu choice is already atomic.
- The gesture is recognised in the app because it is made of pointer events,
  but it has no local effect — it reaches the authority through the same
  command boundary an MCP client would use.

### 5. Document lifecycle and undo affordances

- File → New, Open, Save, Save As drive K4 through the authority; the window
  title and one dirty indicator derive from both experiment and default-view
  save state.
- Replacing an experiment with unsaved changes asks the user; the equivalent
  MCP request carries that decision as an explicit field (ADR 0006).
- Undo and redo submit commands and are labelled with what they will restore,
  from `HistoryStatus`. Disable them honestly when the history is empty.
- Save captures the experiment and authoring-view revisions shown. If either
  has advanced when the write completes, the title remains dirty; the app never
  calls an unqualified "mark clean" hook.
- Plugin inventory refresh is fed to the authority as capability context. An
  unavailable component stays in the tree with its diagnostic and authored
  values; the app neither hides it nor substitutes a schema.

### 6. Verification

- Headless tests over the app's message → envelope mapping, so the mapping is
  covered without a window.
- A windowed smoke check remains manual, as it already is for
  `make smoke-kagami`: `make run-kagami`, create an object, attach a component,
  edit a property as an expression, undo, redo, save, reopen.
- Update `apps/kagami/README.md`: the scene tree is no longer demo state.

## Acceptance criteria

- No demo scene state remains; the scene tree, inspector and title all derive
  from the authority's projection.
- Every experiment change in the UI is an envelope with a command identity and
  actor; no UI code mutates the model directly.
- A rejected edit is reported to the user and leaves the view showing the last
  accepted revision.
- Camera/projection changes in Authoring mode change only the default-view
  revision and mark the file dirty. Selection, expansion, search and hiding,
  plus all playback view changes, remain ephemeral.
- The inspector renders a component contributed by a schema the app has never
  heard of, and offers it for attachment, without an app-side change.
- One viewport drag is one undo entry.
- New/Open/Save/Save As work end to end, including the unsaved-changes prompt
  and the backup-recovery warning path from K4.
- A delayed save completion cannot clear a newer edit, and removing then
  reinstalling a plugin changes availability feedback without changing the
  experiment revision or dirty state.
- `make fmt-check`, `make lint`, `make test`, `make docs`, `make docs-check`,
  and `make smoke-kagami` pass.

## Non-goals

- Run controls, playback, or observation rendering (K-RUN, K-PREVIEW, V-LIVE);
  explicit workspace modes and viewport behaviour are tracked by
  [K11](implement-kagami-viewport-workflows.md).
  The existing play/pause/step toolbar stays honestly labelled as unimplemented
  until a run authority exists.
- The MCP server and its tools — [the MCP task](../kagami-mcp-server.md).
- Catalog management UI beyond invoking instantiation (K5).
- Gizmos or picking. Projection, follow, trajectories and field visualisation
  are tracked by K11.
