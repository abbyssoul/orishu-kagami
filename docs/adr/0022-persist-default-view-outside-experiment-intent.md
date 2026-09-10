# 0022 — Persist a default view outside experiment intent

Status: **accepted**  
Date: **2026-09-07**  
Refines: [ADR 0012](0012-start-with-file-sharing-and-preserve-collaborative-authoring.md),
[ADR 0019](0019-kagami-experiment-document-model.md)

## Context

Camera and visualization choices do not affect physics and must not enter the
experiment revision or workload identity. Some of those choices nevertheless
need to survive save and reopen. In particular, reopening an orthographic scene
as perspective is needlessly disorienting. Calling all saved file content
"experiment intent" would force a false choice between reproducible physics and
a stable user experience.

Kagami also needs an unmistakable distinction between editing initial
conditions and observing/replaying an immutable run.

## Decision

The saved Kagami file is a versioned envelope with two independently interpreted
sections:

- authoritative **experiment intent**, owned and revisioned by the document
  authority; and
- an optional **default view**, owned by the client and ignored by workload
  compilation, Orishu and headless document authorities.

The authoring default view preserves at least projection mode, camera pose and
orbit/focus target so reopening an experiment returns to the view the user
saved. It may also contain versioned, bounded visualization-layer defaults or a
follow target. Projection is explicitly either perspective or orthographic. A
follow target uses a stable object identity and degrades to an unfollowed view
when that identity is absent. Window layout and selection remain application
preferences rather than experiment-file content.

The default view also owns **scene scale**: the positive, bounded length in
metres represented by one viewport unit. It maps canonical SI camera and
geometry values into render space and defines scale-relative camera reach;
it never rescales stored physical quantities, the simulation domain, or solver
resolution. It follows the same view revision, persistence and ephemeral
observation rules as projection, and is outside shared MCP parity. Uniform
scaling is not a guarantee of distant-origin precision. Implementation and
compatibility are tracked by [K14](../tasks/kagami/choose-scene-scale.md);
this extends the view section, not the experiment schema.

Changing the authoring view does not advance the experiment revision and is not
an experiment undo entry, but it does advance a separate view revision and
marks the file dirty. The user sees one file-modified indication derived from
either unsaved experiment intent or an unsaved authoring view. Save captures
both exact revisions and may mark the file clean only if both still match when
IO completes. Sharing the file shares its opening view intentionally, not a
synchronized camera.

Observation/replay presentation is different. Entering it copies the saved or
current authoring view as the initial camera. Camera, projection, follow,
playback and layer changes made while observing are ephemeral and never dirty
the experiment file, run, result or server-owned observation stream. Another
observer begins from the shared experiment default view when it is available,
or from a deterministic fit-to-domain view when only a run reference is
available; it never inherits a previous observer's camera.

Kagami exposes two explicit workspace modes:

- **Authoring** edits the experiment's initial conditions and exposes document
  commands, including undo and redo.
- **Observation/replay** displays one local or remote run and exposes playback,
  subscriptions and visualization. It exposes no authoring or undo controls.

"Edit initial conditions" is an explicit transition back to Authoring. Leaving
a local preview stops that preview and restores the authored initial scene.
Leaving a remote observation detaches the window from it; stopping the cluster
run is a separate privileged action and is never implied. Opening a document
always starts in Authoring and never resumes a run merely because presentation
defaults were saved.

The mode restriction belongs to the application/session workflow, not to the
document authority. While this Kagami session is in Observation/replay, neither
UI nor MCP authoring is routed to the authority. An MCP request that wants to
edit must explicitly request the same transition to Authoring and receives its
local-stop/remote-detach consequence; it cannot switch modes accidentally as a
side effect of an edit. The authority itself remains independent of run state
and may own a successor draft while another Kagami client observes the run.

## Consequences

- Projection survives reopening without contaminating scientific identity.
- UI copy and controls always make it clear whether an action edits initial
  intent or only the observed run projection.
- MCP authoring continues to target the document authority after the explicit
  mode gate; presentation controls remain outside MCP parity, while MCP run and
  sensor reads follow the selected session/run authority.
- Persistence needs separate validation, versioning and dirty bookkeeping for
  experiment intent and the default-view section.
- An explicit computed-state adoption is a mode-changing workflow. It captures
  and validates the selected observation first; only a successful adoption
  enters Authoring and creates the new revision. Failure leaves the observer in
  Observation/replay. A successful local adoption stops the preview; a remote
  adoption detaches without stopping the cluster run.

## Non-goals

- Persisting credentials, live connections, observation caches, playback
  cursors or an active run.
- Shared cameras, presenter-follow or synchronized playback.
- Making client mode a reason for the document authority to reject an otherwise
  valid command.

## Implementation

Tracked by [Persist and recover experiment documents](../tasks/kagami/persist-experiment-documents.md)
and [Implement Kagami viewport workflows](../tasks/kagami/implement-kagami-viewport-workflows.md).
