# Embed an MCP server in Kagami

Status: **ready** (slices 1–7); slices 8–9 are gated on authorities that do
not exist yet  
Decisions: [ADR 0004](../adr/0004-separate-authoring-commands-from-run-observations.md),
[ADR 0006](../adr/0006-mcp-ui-equivalence.md),
[ADR 0012](../adr/0012-start-with-file-sharing-and-preserve-collaborative-authoring.md),
[ADR 0022](../adr/0022-persist-default-view-outside-experiment-intent.md)
Stories: [External control of Kagami through MCP](../user-stories/kagami/mcp.md)

## Outcome

Kagami embeds an MCP server through which authenticated external clients — AI
agents, scripts, alternative front-ends — command the same live session the UI
uses. The server is a transport over Kagami's authorities, not a second
authority: authoring commands enter the document authority, run control uses
the same run authority the UI uses, and presentation state stays local to the
Kagami window (ADR 0006).

The first deliverable is the server lifecycle — enabling and disabling in the
running app, `--mcp` at startup, credential handling, and connected-client
visibility — plus transport, authentication, and one honest tool. Domain tools
grow with the authorities they command; full parity with "everything a user can
do in the UI" is gated on the document and run authorities, not on the
transport work.

This embedded, loopback-only MCP server is not ADR 0012's future headless
multi-user authoring service. It exercises the same command seam, but does not
define cross-device identity, collaborative permissions, shared undo,
persistence ownership, or a remote document protocol.

## Source assessment

- Field CAD built this end-to-end and is the implementation reference:
  `../field-cad/docs/mcp-plan.md` (phased plan with corrections found during
  implementation), `../field-cad/crates/fieldcad-mcp` (transport, bearer
  middleware, connection counting, tools, tests), and
  `../field-cad/apps/fieldcad-desktop/src/mcp.rs` (embedding into a running
  GUI app).
- Per [migration.md](../migration.md), Field CAD's server and MCP transport
  are not adopted as a second compute or control plane. Rebuild the transport
  against Kagami's authorities; remove Field CAD names rather than preserving
  compatibility aliases.
- Transfer Field CAD's recorded lessons rather than rediscovering them:
  - The embedding shape: a dedicated OS thread running a minimal
    current-thread tokio runtime, sharing the session through
    `Arc<std::sync::Mutex<...>>` with the synchronous UI loop; a bounded
    ready-channel reports bind success or failure; a fatal-channel reports a
    server that died after a successful bind; `CancellationToken` stops the
    server on disable.
  - The `Mutex` is `std::sync`, and locks use
    `unwrap_or_else(PoisonError::into_inner)`: a panic reachable from one MCP
    request must not crash the app on its next frame.
  - Bearer-token auth is required for HTTP even on loopback; the token is a
    fresh UUID per enable, shown masked with a copy action, and never
    persisted.
  - Connection counting wraps the rmcp session table
    (`LocalSessionManager`), constructed before the server starts so the UI
    can hold it immediately; it is non-blocking (`try_read`) and can lag a
    client that vanished uncleanly — the UI states that caveat.
  - Wire-protocol smoke tests are mandatory: Field CAD's in-process tests
    missed a serialization bug that only the real JSON-RPC wire path
    triggered.
  - Quick disable-then-re-enable can transiently fail with "address in use";
    the error names that likely cause instead of a raw OS error.
  - Once two command sources share one session (slice 8), one owner mints
    command identities and each submission registers its own completion
    waiter under the same lock — Field CAD found command completions could
    otherwise be stolen or cross-delivered between transports.
  - No tool ever advances wall-clock time; that is the app's frame loop, not
    a client decision.
- `rmcp` (the official Rust MCP SDK) remains the SDK choice. Field CAD pinned
  3.1.x; 3.2.0 is current. Verify the chosen version against its vendored
  source and tests before building on it, especially the Streamable HTTP
  server API and the session-table access the connection count needs.

## Implementation slices

### 1. Verify the SDK and decide dependencies

- Verify `rmcp` 3.2.0 (fall back to Field CAD's proven 3.1.x if needed):
  Streamable HTTP server support, session-table visibility for connection
  counting, Host-header rebinding protection, and cancellation-token
  integration.
- Add the required dependencies to the workspace and record their
  justification per `GUIDELINES.md`: `rmcp` (server, streamable HTTP,
  macros), `tokio`, `tokio-util`, `axum` pinned to match rmcp's own test
  pins, `uuid` v4, `serde`/`serde_json`. Keep them attached to `apps/kagami`
  only; no library crate under `crates/` gains them.
- Update `Cargo.lock` and keep it committed.

### 2. Create the session seam and module skeleton

- Add an app-local `apps/kagami/src/mcp/` module. Do not create a separate
  workspace crate: the server has exactly one caller, and the migration
  admission test says a module stays inside its only caller until a second
  one exists.
- Define the session handle the server commands: an
  `Arc<std::sync::Mutex<SessionState>>`. `SessionState` starts minimal —
  app identity and the open-document status — and is shaped so the document
  authority (slice 8) and run authorities (slice 9) plug in without
  transport changes. This mutex is an app-local adapter mechanism, not the
  document authority's public/domain representation: experiment state,
  commands, revisions, and outcomes must remain serializable and independent
  of UI objects and process-local pointers. Do not expose the demo scene tree
  as domain state.
- Implement one initial tool, `kagami_status`, returning app and session
  information. It exists to prove the full tool path (schema, invocation,
  structured result) end-to-end before domain tools depend on it.

### 3. Build the transport and authentication layer

- Adapt Field CAD's transport layer to Kagami: `bind_http` followed by a
  long-running `serve_http`, so the caller learns bind success and the bound
  address without waiting on the serve loop.
- Serve Streamable HTTP mounted at `/mcp` on loopback only
  (`127.0.0.1:8642` by default, matching Field CAD so one agent
  configuration works for both products). Refuse non-loopback binds
  outright; exposing beyond the local machine is a separate decision.
- Require `Authorization: Bearer <token>` on every request, compared in
  constant time. Generate a fresh UUID token per enable; never persist it.
- Track connected sessions with a caller-constructed connections handle over
  rmcp's session table; expose a non-blocking count.
- Stop the server cooperatively with a cancellation token and graceful
  shutdown; outstanding requests fail with a clear result rather than
  hanging.
- HTTP only in this task: no stdio and no Unix socket transport (the server
  is embedded in the GUI app, not spawned by a client).

### 4. Embed the server in the app and add `--mcp`

- Embed the server in the running app following Field CAD's desktop pattern:
  enabling spawns a dedicated OS thread with its own minimal current-thread
  tokio runtime, sharing the session handle; the UI thread waits at most a
  bounded timeout (~2 s) for the bind result and never blocks on the serve
  loop afterwards.
- Add a `--mcp` flag to `kagami`: the app opens normally and the MCP server
  is enabled from startup. The flag and the in-app toggle run the same server
  under the same rules.
- Report the endpoint and token of a startup-enabled server at startup
  (console/log), so a workflow without a visible window can hand them to a
  client.
- If the server cannot bind — at startup or from the UI — Kagami reports the
  failure with a reason, leaves MCP disabled, and remains fully usable. A
  failed MCP start never blocks authoring or visualization.

### 5. Build the UI for status, enable, disable, and connections

- Model the server state in the app model: disabled, running (endpoint,
  masked token, connected-client count), or failed with a reason.
- Provide an explicit enable action and a disable action. Enabling and
  disabling never modify the open experiment, a run, or the cluster
  connection; disabling is not a run control and a running simulation keeps
  running.
- Show the connected-client count while enabled, including the explicit zero
  case, with the caveat that a client that vanished without closing its
  session may remain counted until the server notices.
- If clients are connected when the user disables, name how many clients
  lose access and require explicit confirmation before completing.
- Show the token masked by default with a copy action. Re-enabling after a
  disable generates a new credential; old credentials stop working.
- Keep the displayed state honest: poll the server non-blockingly (status
  and liveness) on a short interval while enabled, and move the state to
  failed if the server thread stopped after a successful bind.

### 6. Test the lifecycle end-to-end

- Unit tests: token freshness per enable; non-loopback refusal; bearer
  accept/reject; connection count 0→1 on a real `initialize` handshake;
  disable cuts off further requests; re-enable accepts only the new token.
  Field CAD's `crates/fieldcad-mcp/tests/connections.rs` is the template.
- Wire smoke test over real HTTP with the token:
  `initialize → notifications/initialized → tools/list → tools/call`
  against `kagami_status`. This is mandatory, not optional (source
  assessment, wire-protocol lesson).
- Hostile-input tests per `GUIDELINES.md`: missing or wrong token, malformed
  JSON, and oversized bodies produce clear bounded errors and never crash or
  hang the app.
- Windowed app-level verification (enable/disable from the real UI, an agent
  driving the live session) remains a manual step — CI cannot drive the
  window — as it was in Field CAD.

### 7. Document usage

- Update `apps/kagami/README.md`: `--mcp`, the default endpoint, token
  handling, and the loopback-only security posture.
- Do not document unimplemented tools; the tool surface documentation grows
  with slices 8–9.

### 8. Authoring, inspection, and document-lifecycle parity (gated)

Gated on the document authority: the experiment model with typed commands,
validation, revisions, identity, and undo. That authority is decided by
[ADR 0019](../adr/0019-kagami-experiment-document-model.md) and built by the
[Kagami capability programme](./kagami/README.md) — specifically
[K3](./kagami/implement-document-authority.md) for the command envelope,
guards, bounded replay, shared undo, and preflight validation;
[the landed-boundary follow-up](./kagami/harden-document-boundaries.md), which
has landed the serializable adapter values (`kagami_session::wire`) and
actor/payload-bound replay this slice converts through; and
[K4](./kagami/persist-experiment-documents.md) for the new/open/save lifecycle
this slice must expose. Variable and expression commands arrive with
[K2](./kagami/integrate-document-variables.md), which lands
[the shared variables subsystem](./migrate-and-integrate-variables-subsystem.md)'s
experiment integration.

- Expose the authoring surface through tools: create, modify, and delete
  world objects and their physical properties; computational domain and
  discretization; simulation parameters; active fields and models; initial
  conditions; requested observations; variable definitions and
  property expressions.
- Route every tool through the authoritative command path with command
  identity, actor provenance, and optional base-revision guarding; report
  the document decision (accepted revision, or rejection with a domain
  reason), never mere transport delivery.
- Apply K11's workspace-mode gate before routing authoring: in
  Observation/replay, a caller must explicitly request Edit initial conditions
  and receive the local-stop or remote-detach outcome. An edit cannot switch
  mode implicitly, and the document authority itself remains run-agnostic.
- Add document lifecycle: new, open, save. Where the UI would ask the user
  (for example: unsaved changes before replacing the open experiment), the
  MCP caller supplies that decision explicitly in the request.
- Save captures the current authoring default view and its revision alongside
  the experiment, but MCP cannot move the camera or mutate presentation state.
- Add reads for the full experiment state at its current revision and the
  capability-discovery tools (schemas, models, parameter names, units, valid
  ranges), self-described through MCP's own discovery mechanism.
- Add the parity test: the same command script submitted through the UI path
  and through MCP produces the same accepted revisions and state projection.
  Field CAD's parity test and its corrections (`mcp-plan.md` phase 7) are the
  template; keep the test where the tool methods are visible.
- Object-catalog discovery, CRUD, validation, reload, and instantiation follow
  [the catalog task](./implement-kagami-object-catalog.md). Catalog operations
  command its catalog authority; instantiation commands the document authority,
  under the same UI/MCP parity rules.

### 9. Run-control parity (gated)

Gated on the run authorities: an in-process local run and Kagami's
authenticated Orishu client session for cluster runs.

- Expose local run controls (play, pause, step) with exactly the UI's
  semantics; a run started through MCP is the run the UI shows, not a
  parallel one.
- Expose bounded probe/sensor and exact-trajectory reads only through
  K-OBSERVATION's retained authority. Complete field snapshots and explicit
  region/channel/LOD projections carry the same provenance as UI observations;
  MCP never samples rendered pixels or private solver memory.
- Expose cluster submission and control under the UI's rules: the same
  completeness checks, explicit caller-supplied confirmation to replace an
  active workload, the same privilege requirements, and the same cluster
  session — MCP never opens a second connection path.
- Await asynchronous command completion with Field CAD's `submit_and_wait`
  design: one owner mints command identities; each submission registers its
  completion waiter under the same lock.
- Expose run-state and observation reads with full provenance; observing is
  read-only and never dirties the document. Probe/sensor queries use
  [K10](./kagami/compile-and-query-observation-instruments.md)'s bounded exact-
  boundary and time-range surface so an agent can sense the simulated scene
  without camera access or a private resampling path.

## Acceptance criteria

- With MCP disabled (the default), no port is bound and no request can reach
  the session.
- Enabling from the UI and starting with `--mcp` both produce a running
  server on a loopback endpoint requiring a fresh bearer token; the endpoint
  and token are visible to the user (masked, copyable) and reported at
  startup for the flag path.
- The UI shows disabled, running (with endpoint and connected-client count),
  and failed states; the count includes the explicit zero case and the
  stale-session caveat.
- Disabling with connected clients names the consequence, requires
  confirmation, and afterwards those clients' requests fail with a clear
  result; reconnecting requires the newly enabled server's new credential.
- Enabling, disabling, and serving never modify the open experiment, a run,
  or the cluster connection; disabling does not stop a running simulation.
- Missing, malformed, or oversized requests and wrong tokens produce bounded
  clear errors; none crash or hang the app.
- The wire smoke test passes against a real HTTP connection with token
  authentication.
- Slices 8–9 are not simulated or stubbed against the demo scene tree; their
  tools land only when the authorities they command exist.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Exposing the server beyond local transports; remote (cross-device) access
  needs its own threat-model decision first.
- Implementing the headless multi-user document service from ADR 0012 by
  widening this embedded MCP listener.
- A standalone MCP server binary, and stdio or Unix-socket transports.
- Persisting the credential (for example, in a keychain) across app sessions.
- A separate non-MCP HTTP/REST surface; the original "REST API" requirement
  is resolved as MCP over HTTP (ADR 0006).
- Controlling presentation state (camera, selection, layout) through MCP;
  it is client-local to the Kagami window.
- Wiring MCP to the prototype demo scene tree as if it were the experiment
  model.
- Adopting Field CAD's `fieldcad-server`/`fieldcad-mcp` crates wholesale;
  they are references, not sources.
