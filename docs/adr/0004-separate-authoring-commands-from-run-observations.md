# 0004 — Separate authoring commands from run observations

Status: **accepted**  
Date: **2026-09-04**

## Context

Kagami receives state-bearing input from several sources. A person edits an
experiment through the UI; an external agent may edit it through an MCP server
exposed by Kagami; and a local solver or Orishu cluster publishes computed
states for a run. Treating all of these inputs as equivalent mutations of one
scene would obscure who owns each state, bypass document validation and undo,
and allow transient or late compute output to rewrite authored intent.

The experiment and a run are distinct entities. The experiment is editable
intent owned by Kagami. Submission compiles one experiment revision into an
immutable workload. A local solver or Orishu then owns the resulting run and
publishes observations at committed simulation boundaries. Computed state can
inform later authoring, but it is not itself an edit to the experiment.

Kagami also follows a functional-core, imperative-shell design. UI, MCP,
network, filesystem, and solver adapters are IO boundaries; none should gain a
private mutation path into authoritative application state.

## Decision

Kagami has separate authorities for authoring and execution state. These are
logical ownership boundaries, not requirements that both authorities live in
the UI process:

- The **document authority** owns the current experiment revision, validation,
  dirty state, and undo/redo history. Every source of authored change,
  including the UI, MCP clients, undo, redo, and any future remote
  collaboration client, submits a typed experiment command to the same
  authoritative transition. Adapters never mutate the experiment directly.
- Each command expresses intent rather than replacement internal state. It is
  accepted atomically as a new experiment revision or rejected with a domain
  reason while the last valid revision remains current.
- Command envelopes carry a stable command identity and actor provenance. A
  caller that requires optimistic concurrency also supplies the experiment
  revision against which it formed the command. Duplicate command identities
  are idempotent, and stale guarded commands are rejected rather than silently
  applied to another revision.
- The **run authority** is the local solver for a local run and Orishu for a
  cluster run. It publishes immutable, provenance-bearing observations. Kagami
  validates and folds those observations into a run projection or cache; an
  observation never enters the document command path and never mutates the
  experiment.
- Local and remote run adapters implement the same observation semantics.
  Renderer and UI code consume document state, run projections, and local
  presentation state without reading or mutating solver-owned memory.
- Turning computed state into authored intent is an explicit experiment
  command, such as adopting a chosen run boundary as new initial conditions.
  Acceptance creates a normal validated experiment revision and undo entry and
  retains the source run, boundary, and workload provenance.
- Presentation changes such as camera, selection, visibility, and window
  layout remain local presentation messages. They are neither experiment
  commands nor run observations.

The intended flow is:

```text
UI ---------+
             +-- experiment command --> document authority --> revision
MCP adapter -+                              |
                                            | compile and submit
                                            v
                                     immutable workload
                                            |
local solver / Orishu -- observation --> run projection --> presentation
```

IO adapters decode, authenticate, bound, and translate external input, then
deliver values to pure state transitions. Effects such as saving, submitting,
sending an MCP response, or receiving the next observation remain in the
imperative shell and return their outcomes as messages.

The initial authority is hosted inside each Kagami process. Its experiment
model, typed commands, revision identities, and decisions do not depend on UI,
renderer, or process-local object identity, so the same authority can later be
hosted by a headless collaboration service. The local undo/redo history in this
decision does not define shared multi-user undo; that requires attributable
commands and an explicit collaboration protocol. See
[ADR 0012](0012-start-with-file-sharing-and-preserve-collaborative-authoring.md).

## Consequences

- UI and MCP edits share validation, revisioning, identity preservation,
  dirty-state tracking, undo/redo, and acceptance/rejection semantics.
- An MCP tool result reports the document decision, not merely successful
  transport delivery. MCP handlers do not receive mutable document access.
- Command identity, actor, base revision, acceptance/rejection, and resulting
  revision are available for diagnostics and future audit history.
- Running, observing, reconnecting to, or replaying a simulation cannot dirty
  or silently alter the experiment document.
- A submitted workload remains tied to the exact experiment revision from
  which it was compiled; later edits require a new submission.
- The renderer may compose authored geometry, a selected run observation, and
  presentation state, but that visual composition transfers no ownership
  between them.
- Work is required to design explicit adoption commands for each supported
  computed-to-authored workflow. This ceremony is intentional because the
  operation crosses an authority boundary.
- Moving the document authority out of process changes deployment, persistence,
  authentication, and concurrency policy, but must not create a second
  experiment model or mutation path.
