# Implement the `orishu-monitor` TUI shell

Status: **implemented — shell slice only; live client integration deferred to
[operator API integration](integrate-orishu-monitor-operator-api.md)**

Roadmap package: **P-MONITOR**

Related stories: [Cluster administration](../user-stories/orishu/cluster-admin.md),
[Workload management](../user-stories/orishu/workload.md), and
[Worker administration](../user-stories/orishu/worker-admin.md).

## Outcome

`orishu-monitor` starts as a robust full-screen terminal application with a
clear information hierarchy, keyboard navigation, help, empty and unavailable
states, and terminal-safe shutdown. Its application state and rendering are
testable without a real terminal or worker.

This is intentionally a foundation slice. It does **not** connect to an Orishu
worker or claim that operational data is live. A later P-MONITOR task will map
accepted client projections into the shell after the relevant worker routes and
public client contracts stabilize.

## Product destination

`orishu-monitor` is the interactive administrative counterpart of
`orishuctl`, not merely a dashboard. When P-MONITOR is complete, an operator
can choose the CLI or TUI for every supported story-level administrative read
and mutation, with the same authorization, validation, operation identity,
retry, audit, and outcome semantics.

The deliberate exception is raw formation-admission secret handling. The TUI
must not reveal, copy, or print the join token or join-material secret. Operators
use the CLI's private-file workflow to retrieve that material; a later TUI join
flow may consume an already prepared private file without displaying its secret.
A TUI token-rotation flow is out of scope until a secure file-output contract is
accepted, because rotation returns new secret material that must not be lost or
placed in terminal history.

This destination constrains the shell's extensibility, but does not add any
live views or actions to the present slice.

## Gap this task closed

`apps/orishu-monitor` printed `Hello, world!`. The repository has legacy client
models and an HTTP client with a broad method surface, while the worker
implements only the evolving cluster-formation subset of the client protocol.
Method presence in `crates/orishu` is therefore not evidence that a route is
implemented or that its model is the accepted long-term projection.

Implementing every screen against those types would have coupled the TUI to API
work still owned by N-FORMATION and O-API-SHAPE/O-CLIENT. Terminal lifecycle,
application navigation, layout, rendering states, and input semantics could be
implemented and reviewed independently, and this task did exactly that without
inventing a monitor-specific protocol or mock server.

The shell now lives in `apps/orishu-monitor` as a functional core
(`model`/`message`/`update`/`keys`/`view`) behind an imperative shell
(`runtime`, and the private `terminal` session guard). It has no `orishu`
dependency. The gap that remains is live integration, which is
[its own backlog task](integrate-orishu-monitor-operator-api.md) and is still
gated on accepted projections.

## Scope and design constraints

- Keep the binary name `orishu-monitor`. It is the planned interactive terminal
  operator client alongside the scriptable `orishuctl` client.
- This slice is read-only. Do not add cluster mutations or confirmation flows,
  and do not claim that end-state `orishuctl` parity has already been delivered.
- Keep the implementation inside `apps/orishu-monitor`; do not create a shared
  crate for shell concepts that have only one consumer.
- Use a functional core and IO shell. User input and terminal events become
  typed messages; a deterministic update function owns navigation and UI state;
  terminal setup, event polling, drawing, and cleanup stay at the edge.
- Presentation models are app-local and must not be serialized or presented as
  protocol/domain authority. Do not copy the legacy client resource hierarchy
  into new monitor DTOs merely to prepare for hypothetical endpoints.
- All panels that await integration say so explicitly. Never render example,
  default, or zero-valued data as though it came from a worker.
- The event loop and any internal queues are bounded. A resize or input burst
  must not create unbounded retained work.
- Choose maintained Rust TUI and terminal backends appropriate for the existing
  workspace (the expected choice is `ratatui` with `crossterm`). Put genuinely
  shared dependency declarations in `[workspace.dependencies]`, keep versions
  reproducible, and record the dependency rationale in the implementation
  handoff.
- Remove the monitor's direct `orishu` dependency if the shell no longer uses
  it. Do not retain or call it merely to make the future integration look
  started.

## Bounded implementation slices

### 1. Establish the application core

- Replace the placeholder with a small binary entry point and app-local modules
  for the model, messages/update logic, terminal runner, and view.
- Model at least the selected section, focus/selection needed by that section,
  help visibility, terminal size class, integration availability, and exit
  intent. Do not put terminal handles or client objects in the model.
- Define typed messages for supported keys and resize events, plus ticks only
  if this slice has a concrete rendering need for them. Keep update logic
  deterministic and free of terminal or network IO.
- Keep effects explicit. This slice needs only effects actually performed by
  the shell, such as exiting; do not design an abstract networking framework in
  anticipation of the follow-up.

### 2. Own terminal lifecycle safely

- Acquire raw mode, alternate-screen, and cursor state through an owned session
  guard that rolls back partial initialization. Restore them on normal exit and
  every returned error after terminal entry; do not scatter cleanup across
  branches.
- Refuse a non-interactive input/output stream clearly before changing terminal
  state. `--help` and `--version` remain usable without a TTY.
- Handle `q` and `Ctrl-C` as exit requests, `?` as help, `Esc` as close/back,
  and conventional arrow plus `j`/`k` navigation. The help view is the
  authoritative list of implemented keys.
- Redraw on resize and render a compact explanatory fallback at very small
  terminal sizes instead of panicking or producing invalid layout constraints.
- Avoid a busy loop. Event polling/ticks must have an explicit cadence and
  should not consume a CPU core while idle.

### 3. Render the shell and honest states

- Provide stable top-level navigation for **Overview** and **Members**, with a
  **Help** overlay. These names reflect durable operator concepts without
  specifying the eventual wire resources.
- Show application identity and the active section consistently, plus a status
  area for informational/error text.
- Overview and Members render useful empty/unavailable states. In this slice,
  the default state must state that live worker integration is not included;
  it must not imply an attempted connection or authentication failure.
- Keep rendering a projection of the model. Rendering must not mutate
  selection, consume events, initiate refreshes, or perform IO.
- Use theme/style tokens rather than scattering colour choices, and retain
  readable distinctions when colour support is limited.

### 4. Add focused verification and operator documentation

- Unit-test navigation, help/back behavior, exit handling, selection bounds,
  and resize transitions through the same update function used in production.
- Render representative normal, unavailable, empty, help, and minimum-size
  states with a headless test backend. Prefer semantic assertions over brittle
  full-frame snapshots where either would prove the behavior.
- Update `apps/orishu-monitor/README.md` with the implemented keys, current
  shell-only status, how to run it, and the explicit absence of worker API
  integration.
- Keep package metadata truthful: describe an experimental operator TUI shell,
  not a completed administration client.

## Acceptance criteria

- Running `orishu-monitor` in a terminal opens the full-screen shell; Overview,
  Members, and Help are reachable using only documented keys.
- The default UI clearly reports that worker integration is not part of this
  slice. No fixture or placeholder value is presented as live cluster state.
- Quit and error paths after terminal entry restore the terminal. Resize and
  minimum-size rendering do not panic.
- Core navigation/update tests run without a terminal, and the principal view
  states render through a headless backend.
- No worker route, client protocol, credential flow, mutation, polling loop, or
  streaming contract is added or changed by this task.
- `cargo fmt --all -- --check`,
  `cargo clippy --locked -p orishu-monitor --all-targets -- -D warnings`,
  `cargo test --locked -p orishu-monitor --all-targets`, and
  `make docs-check` pass. The handoff records a manual terminal smoke test or
  states why one could not be performed.

## Explicit follow-up gate

The [operator API integration and parity task](integrate-orishu-monitor-operator-api.md)
is recorded in the backlog. Promote its first live increment only after the
consumed operational projection has an accepted owner, bounded public type,
implemented worker route, and real serialized-path test. The promoted increment
must specify:

- which N-FORMATION or O-CLIENT projections the first live screens consume;
- address, authentication, timeout, refresh, cancellation, and staleness
  semantics shared with `orishuctl`;
- the mapping from client/domain outcomes into app-local presentation state;
- bounded polling or subscription behavior and recovery from unavailable or
  unsupported routes; and
- real-worker integration tests.

Grow and retain an operator-parity matrix across subsequent P-MONITOR tasks.
Every supported `orishuctl` administrative command must map to a TUI view or
action, or to the documented raw-admission-secret exception. Each mutation must
name the same request/outcome contract, confirmation behavior, authorization,
operation identity, exact-retry rule, and audit semantics as the CLI.

Formation Overview/Members is the preferred first live increment once the
membership projection and serving route are accepted. Workload, result,
checkpoint, logs/events, and observation screens wait for their owning
O-CLIENT/runtime/storage/observation contracts. Mutation parity must be
specified in later bounded tasks and must reuse—not reinterpret—the CLI's
authorization, operation-identity, audit, and retry semantics.

## Non-goals

- Connecting to a worker, parsing credentials, polling, subscriptions, or
  handling live protocol data.
- Designing or changing the Orishu client protocol or treating legacy client
  types as accepted merely because they compile.
- A mock worker, a monitor-specific service abstraction, or a comprehensive
  operational fixture schema.
- Cluster or workload mutations, destructive-action confirmation, join-token
  handling, or delivery of `orishuctl` command parity **in this shell slice**.
- Workload/result/checkpoint/log/event/observation detail screens.
- Metrics, alerting, Prometheus, or OTLP workflows, which belong to
  P-OBSERVABILITY and P-OBS-DOCS.
- Experiment authoring or scientific visualization, which belong to Kagami.

## Dependencies and roadmap placement

The shell slice has no N-FORMATION or O-CLIENT dependency and may proceed as a
side task while peer transport and formation are in progress. It is enabling
work, not an M4 exit criterion and not evidence that the client API is
complete.

The first live Overview/Members adapter depends on the accepted N-FORMATION
membership projection and its implemented client route. Later operational
views depend on O-API-SHAPE/O-CLIENT and the runtime, storage, or observation
authority that owns their data.
