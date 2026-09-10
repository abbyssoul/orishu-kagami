# Kagami capability programme

Status: **K1, K3, the boundary follow-up, K4 and K14 are implemented**; K2, K5,
K6 and K11 have implemented slices with named gates; K7 is partial; K8–K10 and
K12–K13 are specified

Work packages: **K-DOCUMENT, X-COMPOSITION, K-OBSERVATION, K-VIEW,
X-EMITTER, and X-FIELDS** in the [implementation roadmap](../../roadmap/README.md)  
Decision: [ADR 0019](../../adr/0019-kagami-experiment-document-model.md)

This directory holds the bounded tasks that build Kagami's authoring spine and
the cross-lane capabilities that consume it.
Each file is independently assignable: it names its owning boundary, the
contracts it consumes, its acceptance criteria, and its non-goals.

`apps/kagami` currently renders a demo scene tree that
[migration.md](../../migration.md) marks as placeholder. Until this programme
lands, catalog instantiation, MCP authoring parity, workload compilation, and
local preview all have nothing authoritative to command.

## Outcome

The authoring spine begins with two crates and one adapter change:

```text
apps/kagami  (imperative shell: Iced UI, MCP adapter, file dialogs, run adapters)
     |  command envelope                      ^  read projection / change events
     v                                        |
crates/kagami-session   -- the document server: the sole mutation path
     |  update(model, commands) -> candidate  ^  typed rejection
     v                                        |
crates/kagami-document  -- pure sans-IO model, commands, transition, history
     |
     +-- orishu-variables  (the one expression engine; dimensions and units)
     +-- kagami-catalog    (materialised object candidates; component schemas)
```

## Tasks

| # | Task | Status | Gated on |
| --- | --- | --- | --- |
| K1 | [Implement the experiment document model](implement-experiment-document-model.md) | Implemented as `crates/kagami-document` | ADR 0019 |
| K1/K3 follow-up | [Harden the landed document and session boundaries](harden-document-boundaries.md) | Implemented; capability projection, schema adoption, replay binding, save acknowledgement, wire boundary | K1, K3 |
| K2 | [Integrate document variables and expressions](integrate-document-variables.md) | Slices 1–3 implemented; slice 4 needs X-PLUGIN and K5 symbol sources | K1; [S-VARIABLES](../migrate-and-integrate-variables-subsystem.md) slice 2 |
| K3 | [Implement the document authority](implement-document-authority.md) | Implemented as `crates/kagami-session` | K1 |
| K4 | [Persist and recover experiment documents](persist-experiment-documents.md) | Implemented; ADR 0022's default-view section landed with K11 slice 2 as format version 2 | none |
| K5 | [Instantiate catalog templates into the document](instantiate-catalog-templates.md) | Slices 1–2 implemented: the command, the bridge, no tracking link; slice 3's live-dependency half needs K2 slice 4 | [K-CATALOG](../implement-kagami-object-catalog.md) |
| K6 | [Adopt the document authority in the Kagami app](adopt-document-authority-in-kagami.md) | Slices 1–3, 5–6 implemented, including the view revision; gestures are unblocked but not implemented | X-PLUGIN schema inventory for a real registry |
| K7 | [Capture the missing authoring user stories](capture-authoring-user-stories.md) | Partially implemented; core capability stories captured | none |
| K8 | [Model requested observations](implement-requested-observations.md) | Specified; blocked | K4; X-PLUGIN observation-channel schema contract (K7 probe story satisfied) |
| K9 | [Define composed object execution](define-composed-object-execution.md) | Specified; topology accepted, implementation blocked | X-PLUGIN, S-WORKLOAD, O-WASM, X-FIELDS |
| K10 | [Compile and query observation instruments](compile-and-query-observation-instruments.md) | Specified; blocked | K8, S-WORKLOAD, S-OBSERVE, K-RUN/K-PREVIEW |
| K11 | [Implement Kagami viewport workflows](implement-kagami-viewport-workflows.md) | Slices 1–2 implemented: mode machine and gate, orthographic projection, persisted default view; nothing enters Observation/replay yet | K-RUN/K-PREVIEW to enter observation; K-RUN/S-OBSERVE for follow and fields; V-REPLAY for trails |
| K12 | [Implement particle emitters](implement-particle-emitters.md) | Specified; cross-lane | K5, K9, S-WORKLOAD, O-RUNTIME; N-CLUSTER for distributed proof |
| K13 | [Define fields and computational-model selection](define-fields-and-model-selection.md) | Specified; cross-lane | K-DOCUMENT, X-PLUGIN, S-WORKLOAD, S-OBSERVE |
| K14 | [Choose the scene scale](choose-scene-scale.md) | Implemented; `SceneScale`, scale-relative camera bounds, `defaultView` version 2, view control. The shared prefix-selecting formatter stays S-VARIABLES' | none |

K7's remaining documentation work can proceed alongside any code task. K1, K3,
the [boundary follow-up](harden-document-boundaries.md), S-VARIABLES' dimension
layer, K2's document graph, and K4 in full have landed. K5's
catalog-instantiation command and bridge, K6's non-gesture app adoption, and
K11's workspace modes and default view have also landed. Their remaining slices
are explicit: plugin/catalog symbol sources for K2/K5, a real X-PLUGIN schema
inventory and the now-unblocked gesture bracket for the app, and K-RUN/K-PREVIEW
before anything can enter Observation/replay.

K9–K13 preserve the Field CAD capabilities that cross the document boundary:
component-composed execution, compiled observation instruments, explicit
authoring/observation viewport workflows, workload-captured emitters, and
explicit field-family/model selection. They
do not block K4's basic persistence mechanics, except that K4 must include the
separately owned default-view envelope required by K11.

K14 has landed. It retired the room-sized camera bounds K11 shipped — under
which a 2 nm object could not be approached and a planetary orbit could not be
framed — by expressing the camera's reach in render units and deriving the
metre-space limit from a `SceneScale`. It was also the first extension of the
separately versioned default-view section, which is what let a presentation
field be added without moving the file's `FORMAT_VERSION`: the section is at
version 2 and the envelope is unchanged. Two of its consequences reach other
tasks — `AuthoringView` is now the validated unit rather than `CameraPose`, so
K6's gesture work should build on that; and every later object, field and trail
consumer must convert through `SceneScale` at the render boundary rather than
casting metres to `f32`.

The MCP server's slice 8 now has a landed authority core, expression-capable
document commands, document lifecycle, and the serializable representation and
replay binding an adapter converts through (`kagami_document::wire`,
`kagami_session::wire`). The MCP adapter and tools remain implementation work;
they are no longer blocked on K2's core document graph or K4's experiment
persistence.

## What this programme unblocks

| Downstream | Currently gated on | Released by |
| --- | --- | --- |
| [Kagami MCP server](../kagami-mcp-server.md) slice 8 — authoring and document-lifecycle parity | expressions and document lifecycle; the serializable authority boundary has landed | K2, K4 |
| [Kagami object catalog](../implement-kagami-object-catalog.md) — instantiation through a document command | K-DOCUMENT and component schemas | K5 |
| [Catalog values in expressions](../capture-catalog-values-in-expressions.md) | variable namespaces and workload capture contracts | K2, S-WORKLOAD; K5 is not required |
| [Shared variables subsystem](../migrate-and-integrate-variables-subsystem.md) slice 3 — experiment integration | K-DOCUMENT | K2 |
| K-RUN, K-PREVIEW (roadmap registry) | an authoritative, persisted app document | K2, K3, K4, K6, plus their own workload/observation gates |
| X-BUILTINS and O-RUNTIME composed physics | executable component semantics | K9 |
| Probe/sensor UI and MCP reads | authored instruments and run observations | K8, K10 |
| Projection and workspace modes | — released; `kagami_session::{workspace, default_view}` | K11 slices 1–2 |
| Follow, field visualization and trails | observation projection | K11 slices 3–5, K-RUN/S-OBSERVE, V-LIVE/V-REPLAY |
| Atomic- and astronomical-scale viewing | — released; `kagami_session::default_view::SceneScale` | K14 |
| Runtime particle emission | catalog materialization and composed execution | K5, K9, K12 |
| Field-family selection, mutually exclusive models, and field observations | plugin schemas, workload and observations | K13 |

## Source material

Field CAD (`../field-cad`) is the implementation reference, read under
[migration.md](../../migration.md)'s admission test. The mechanics worth
transferring and the decisions that are *superseded* here are recorded in
[ADR 0019](../../adr/0019-kagami-experiment-document-model.md); each task's
"Source assessment" section names the specific files it should read. Field CAD
names, crates, and compatibility aliases are not carried over.

Five divergences are load-bearing and easy to reintroduce by accident:

1. Objects are entities composed from plugin-contributed component schemas —
   not the plain object model of Field CAD ADR 0002.
2. Expression source is retained *on* the property — not in a side-car
   expression document as in Field CAD ADR 0026.
3. There is no catalog tracking link, no propagation, and no compare/apply
   protocol (ADR 0008, ADR 0018).
4. Object visibility is client-local presentation state (ADR 0012) — Field
   CAD's `SetObjectVisible` world command does not transfer.
5. Scene scale is client-local presentation state in the default view
   (ADR 0022) — Field CAD's `scene_scale` world-document field and
   `SetSceneScale` world command do not transfer, and it stays off the MCP
   surface. See [K14](choose-scene-scale.md).

## Conventions

- Every task ends its acceptance criteria with `make fmt-check`, `make lint`,
  `make test`, `make docs`, and `make docs-check`.
- Prefer `cargo … -p kagami-document` / `-p kagami-session` over
  workspace-scoped commands while other lanes are active in this repository.
- Update this index's status column when a task's linked evidence and its
  implementation agree — "specified" is not "implemented".
