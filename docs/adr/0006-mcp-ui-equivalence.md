# 0006 — MCP and the UI are equivalent interfaces to one session

Status: **accepted**  
Date: **2026-09-04**

## Context

Kagami's first interface is its native UI, but the product also serves users
who delegate authoring and run control to external clients: AI agents,
scripts, and alternative front-ends. Field CAD proved this model and recorded
it in its architecture overview and story inventory rather than as an ADR: one
authoritative experiment model with multiple clients, where MCP is a thin
transport over the authoritative session and parity means "parity of
experiment meaning — not parity of mouse gestures." This repository adopts
that decision before building Kagami's MCP surface, so the parity guarantee is
not rediscovered or quietly dropped, and reconciles it with two constraints
Field CAD did not have: Kagami's document/run authority split
([ADR 0004](0004-separate-authoring-commands-from-run-observations.md)) and
the migration rule that Field CAD's server and MCP transport are not adopted
as a second compute or control plane — Orishu owns remote execution.

The original external-control requirement asked for a "REST API." Field CAD
resolved that wording by building MCP over HTTP — one typed surface of tools
and reads — instead of a parallel bespoke REST API. This decision adopts the
same resolution.

## Decision

Kagami exposes external clients one programmatic interface: an MCP server
embedded in the running application, commanding the same live session the UI
uses. In this decision, “multiple clients” means the local UI and one or more
delegated MCP clients attached to that Kagami process. It does not mean a
cross-device, multi-user document service.

- The MCP server is a transport and protocol adapter. It is not a separate
  model, not a second authority, and not a UI-automation layer: nothing
  synthesizes UI input; every request becomes a typed command or read against
  Kagami's existing authorities.
- Parity is parity of experiment meaning, not of gestures. Every authoring
  operation, document operation, run control, and read available in the UI is
  expressible through MCP with the same validation, atomic commit,
  revisioning, command identity, undo, and privilege semantics. Presentation
  state — camera, selection, visibility, window layout — remains client-local
  to the Kagami window and is not part of the shared surface.
- Authoring commands enter the document authority defined in ADR 0004; run
  controls use the same run authority the UI uses (the local solver for local
  runs, Kagami's authenticated Orishu client session for cluster runs). MCP
  never joins cluster membership or opens a second path to execution.
- The server is off by default. It is enabled by a startup flag or an explicit
  action in the running app; each enable generates a fresh credential that is
  required on every request and never persisted; and the server binds only
  local transports (loopback or same-host IPC). Exposing MCP beyond the local
  machine is a separate decision with its own threat model.

## Consequences

- UI edits and MCP edits share validation, revisioning, identity, dirty state,
  undo, and acceptance/rejection; an MCP result reports the authority's
  decision, not transport delivery.
- A user and an agent share one live session; their concurrency resolves under
  the document authority's revision and command-identity rules.
- Parity is a testable property: the same commands submitted through the UI
  path and through MCP must produce the same accepted revisions, run
  decisions, and observations.
- Interactive confirmations (unsaved changes, replacing an active workload)
  become explicit request fields for MCP callers; Kagami never resolves them
  silently.
- Remote, cross-device control is deliberately out of scope until non-local
  exposure is decided; today's MCP clients are same-machine by construction.
- A future headless collaboration service may host the same document authority,
  but its identity, permissions, shared undo, persistence, and remote protocol
  are governed by [ADR 0012](0012-start-with-file-sharing-and-preserve-collaborative-authoring.md),
  not implied by MCP parity or by exposing this embedded server remotely.
