# Kagami capability programme

Status: **K1, K3 and the boundary follow-up implemented**; K2, K4–K13
specified  
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
| K2 | [Integrate document variables and expressions](integrate-document-variables.md) | Specified | K1; [S-VARIABLES](../migrate-and-integrate-variables-subsystem.md) slice 2 |
| K3 | [Implement the document authority](implement-document-authority.md) | Implemented as `crates/kagami-session` | K1 |
| K4 | [Persist and recover experiment documents](persist-experiment-documents.md) | Specified; ready once K2 lands | K2 |
| K5 | [Instantiate catalog templates into the document](instantiate-catalog-templates.md) | Specified; ready once K2 lands | K2, [K-CATALOG](../implement-kagami-object-catalog.md) |
| K6 | [Adopt the document authority in the Kagami app](adopt-document-authority-in-kagami.md) | Specified; blocked | K2, K4, K5; X-PLUGIN schema inventory contract |
| K7 | [Capture the missing authoring user stories](capture-authoring-user-stories.md) | Partially implemented; core capability stories captured | none |
| K8 | [Model requested observations](implement-requested-observations.md) | Specified; blocked | K4; X-PLUGIN observation-channel schema contract (K7 probe story satisfied) |
| K9 | [Define composed object execution](define-composed-object-execution.md) | Specified; topology accepted, implementation blocked | X-PLUGIN, S-WORKLOAD, O-WASM, X-FIELDS |
| K10 | [Compile and query observation instruments](compile-and-query-observation-instruments.md) | Specified; blocked | K8, S-WORKLOAD, S-OBSERVE, K-RUN/K-PREVIEW |
| K11 | [Implement Kagami viewport workflows](implement-kagami-viewport-workflows.md) | Specified; slice-gated | K4/K6 for modes and projection; K-RUN/S-OBSERVE for follow and fields; V-REPLAY for trails |
| K12 | [Implement particle emitters](implement-particle-emitters.md) | Specified; cross-lane | K5, K9, S-WORKLOAD, O-RUNTIME; N-CLUSTER for distributed proof |
| K13 | [Define fields and computational-model selection](define-fields-and-model-selection.md) | Specified; cross-lane | K-DOCUMENT, X-PLUGIN, S-WORKLOAD, S-OBSERVE |

K7's remaining documentation work can proceed alongside any code task. K1, K3
and the [boundary follow-up](harden-document-boundaries.md) their first
downstream consumers exposed are all landed, so the immediate code path is
**S-VARIABLES slice 2 → K2**. K4 and K5 may then proceed independently; K6
integrates both into the app after the simulation-plugin schema-inventory
contract is stable.

K9–K13 preserve the Field CAD capabilities that cross the document boundary:
component-composed execution, compiled observation instruments, explicit
authoring/observation viewport workflows, workload-captured emitters, and
explicit field-family/model selection. They
do not block K4's basic persistence mechanics, except that K4 must include the
separately owned default-view envelope required by K11.

The MCP server's slice 8 now has both a landed authority core and the
serializable representation and replay binding an adapter converts through
(`kagami_document::wire`, `kagami_session::wire`). Its expression tools still
need K2 and its document-lifecycle tools still need K4.

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
| Projection, follow, field visualization and trails | app adoption and observation projection | K11 |
| Runtime particle emission | catalog materialization and composed execution | K5, K9, K12 |
| Field-family selection, mutually exclusive models, and field observations | plugin schemas, workload and observations | K13 |

## Source material

Field CAD (`../field-cad`) is the implementation reference, read under
[migration.md](../../migration.md)'s admission test. The mechanics worth
transferring and the decisions that are *superseded* here are recorded in
[ADR 0019](../../adr/0019-kagami-experiment-document-model.md); each task's
"Source assessment" section names the specific files it should read. Field CAD
names, crates, and compatibility aliases are not carried over.

Four divergences are load-bearing and easy to reintroduce by accident:

1. Objects are entities composed from plugin-contributed component schemas —
   not the plain object model of Field CAD ADR 0002.
2. Expression source is retained *on* the property — not in a side-car
   expression document as in Field CAD ADR 0026.
3. There is no catalog tracking link, no propagation, and no compare/apply
   protocol (ADR 0008, ADR 0018).
4. Object visibility is client-local presentation state (ADR 0012) — Field
   CAD's `SetObjectVisible` world command does not transfer.

## Conventions

- Every task ends its acceptance criteria with `make fmt-check`, `make lint`,
  `make test`, `make docs`, and `make docs-check`.
- Prefer `cargo … -p kagami-document` / `-p kagami-session` over
  workspace-scoped commands while other lanes are active in this repository.
- Update this index's status column when a task's linked evidence and its
  implementation agree — "specified" is not "implemented".
