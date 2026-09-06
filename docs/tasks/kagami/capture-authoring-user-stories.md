# Capture the missing authoring user stories

Status: **partially implemented**; core capability stories captured; remaining
inventory can run in parallel with any code task  
Work package: **K-DOCUMENT** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0004](../../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0006](../../adr/0006-mcp-ui-equivalence.md),
[ADR 0012](../../adr/0012-start-with-file-sharing-and-preserve-collaborative-authoring.md),
[ADR 0019](../../adr/0019-kagami-experiment-document-model.md)

## Outcome

`docs/user-stories/kagami/` describes the authoring outcomes Kagami must
deliver, including the ones Field CAD implemented and this repository has not
yet written down. Without them, later tasks infer requirements from Field CAD
*code*, which is exactly what
[migration.md](../../migration.md) says not to treat as authority.

## Source assessment

Field CAD's story inventory (`../field-cad/docs/user-stories/README.md`) is the
gap list, not the text to copy. Its stories are written for a "modeller" using
an egui desktop; Kagami's are written for the scientist-researcher, the
automation client, the collaborator and the plugin author already defined in
[the Kagami stories README](../../user-stories/kagami/README.md).

Its stories in sections 1–3, 5 and 7 are largely covered by
[authoring.md](../../user-stories/kagami/authoring.md) and
[mcp.md](../../user-stories/kagami/mcp.md). The gaps are:

| Field CAD story | Gap in this repository |
| --- | --- |
| US-26 — validate a proposed transaction before committing it | No story for advisory preflight validation, which is what lets an agent repair input before submitting. K3 builds it. |
| US-30…US-34 — list, add, position, attach, hide and configure measurement instruments; choose what a probe records | No story covers requested observations as authored intent. K8 implements them. |
| US-52/US-53 — value validity (exact, interpolated, or a specific undefined reason) and structured solver diagnostics | Partly implied by the Orishu observation stories; nothing states the Kagami-side requirement that a singularity or boundary is never mistaken for a measurement. |
| US-54 — dynamic-object outcomes and trajectories at an explicit boundary | Captured as best-effort live trails versus authored exact retained trajectories; K8/K10 compile and query the exact form. |
| US-55 — name, retain and compare observations across runs | [authoring.md](../../user-stories/kagami/authoring.md) covers opening a previous run, not comparing two. |
| US-61 — inspect edit history: what undo/redo would restore | Undo/redo is a story; discovering the *label* is not, and ADR 0006 makes it an MCP-parity requirement. |
| US-62 — record and replay a semantic session | No story. Decide explicitly whether this is in scope or a recorded non-goal; do not leave it ambiguous. |
| US-10/US-16 — distinguish what is drawn from what participates in physics | Implied but not stated: an object with no components is a valid, non-simulated scene object. |

The 2026-09-07 refinement captured the core gaps and additional proven Field
CAD outcomes: composed modeled objects and dynamics/field coupling, probes and
attachments, MCP sensor queries, particle emitters, explicit
Authoring/Observation modes, projection and camera follow, field vectors/flow
lines, field-family/model selection, and both best-effort live trails and exact
retained trajectories. US-26 preflight, cross-run comparison and
semantic session replay remain open parts of this task.

## Implementation slices

### 1. Reconcile terminology first

Field CAD's "scene", "world command", "field system" and "modeller" map onto
Kagami's "experiment", "experiment command", "plugin" and
"scientist-researcher". Where no clean mapping exists — Field CAD's *solver
diagnostics*, produced by an in-process plugin — say what the Kagami-side
equivalent is under ADR 0004 (a run observation, not an authoring output)
rather than inventing a hybrid.

### 2. Write the stories

- Use the template in [the stories README](../../user-stories/README.md):
  verb-led title, persona, Given/When/Then, observable acceptance criteria.
- Add measurement-instrument stories to
  [authoring.md](../../user-stories/kagami/authoring.md) under a new
  "Observing the experiment" section, written as *authored observation
  requests* — what the experiment asks to be recorded — kept distinct from a
  client's transport subscription, which is presentation.
- Add the preflight-validation and edit-history-inspection stories, and give
  each an MCP counterpart in [mcp.md](../../user-stories/kagami/mcp.md) where
  ADR 0006 requires parity of meaning.
- Record the deliberate exclusions with their reason: visibility as
  presentation state (ADR 0012), and any Field CAD capability decided
  out of scope, so a later reader finds a decision rather than an omission.

### 3. Close the loop

- Add the ADR 0005 follow-up already recorded in `TODO.md`: the variables and
  expressions stories should state the UI/MCP equivalence explicitly.
- Cross-link each new story to the task that implements it, and update
  [the programme index](README.md) if a story reveals a slice no task owns.

## Acceptance criteria

- Every gap in the table above is either written as a Kagami story or recorded
  as an explicit non-goal with its reason.
- No story is a verbatim port: each names a Kagami persona, Kagami
  terminology, and observable acceptance criteria.
- Every new story that ADR 0006 covers has an MCP counterpart, and every
  presentation-only capability is explicitly excluded from that parity.
- No story asserts that behaviour exists; stories describe outcomes, and the
  roadmap's rule that "documented is not implemented" is preserved.
- `make docs-check` passes.

## Non-goals

- Writing protocol specifications, API contracts, or design documents; those
  are tasks and ADRs.
- Porting Field CAD's suggested MCP surface table as a wire protocol.
- Orishu cluster or worker stories; those live in
  [docs/user-stories/orishu](../../user-stories/orishu/README.md).
