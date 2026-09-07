# Integrate `orishu-monitor` with the operator API

Status: **backlog — do not implement until the named client contracts are accepted**

Roadmap package: **P-MONITOR**

Predecessor: [Implement the `orishu-monitor` TUI shell](implement-orishu-monitor-admin-tui.md).

Related stories: [Cluster administration](../user-stories/orishu/cluster-admin.md),
[Workload management](../user-stories/orishu/workload.md), and
[Worker administration](../user-stories/orishu/worker-admin.md).

## Outcome

An operator can choose `orishuctl` or `orishu-monitor` for every supported
story-level administrative read and mutation. Both clients use the same public
operator API and preserve the same authorization, validation, operation
identity, retry, audit, and outcome semantics. The TUI adds interactive views,
navigation, refresh, staleness reporting, and confirmed actions; it does not
create a second administrative contract.

Raw formation-admission secrets are the deliberate exception. The TUI never
reveals, copies, or prints join tokens or join-material secrets. Retrieval and
export remain a CLI private-file workflow. The TUI may consume already prepared
private material for a future join workflow without displaying it. Token
rotation remains CLI-only unless a separate decision accepts a secure file
handoff for the newly returned secret.

## Why this is backlog work

The TUI shell can be built without Orishu APIs. Live integration cannot yet be
specified safely across the whole operator surface:

- N-FORMATION is still stabilizing the membership projection and minimal
  worker/client routes.
- O-API-SHAPE has not accepted the general resource surface.
- O-CLIENT and the runtime, storage, and observation authorities do not yet
  serve the workload, result, checkpoint, event/log, and live-observation
  contracts needed by later screens.
- The broad legacy method set in `crates/orishu/src/client` does not prove that
  corresponding worker routes exist or that their types are the accepted
  contract.

Implementing against those assumptions would freeze presentation and failure
semantics around provisional APIs. Keep this task visible in the backlog, but
do not treat it as implementation-ready until at least its first increment
passes the promotion gate below.

## Promotion gate

Promote one bounded increment at a time. Before an increment becomes ready,
every resource or mutation it consumes must have:

1. a named authority and accepted, versioned, bounded public request/projection
   type;
2. an implemented worker route with authentication and authorization behavior;
3. a public client method exercised through the real serialized path;
4. documented error, timeout, cancellation, pagination or subscription, and
   staleness semantics as applicable; and
5. real-worker acceptance evidence that `orishuctl` can use as the reference
   adapter behavior.

When a gate is satisfied, refine that increment with the exact types, methods,
routes, limits, and test fixtures before assigning it. Do not promote the
entire task merely because one formation view becomes available.

## Planned increments

### 1. Formation Overview and Members

This is the preferred first live increment after N-FORMATION stabilizes.

- Add endpoint selection, authentication, request timeout, refresh,
  cancellation, and reconnect behavior using the accepted shared client
  contract.
- Map cluster summary and member projections into app-local presentation state;
  do not let protocol types become mutable widget state.
- Distinguish never loaded, loading, current, stale, unsupported,
  unauthenticated/unauthorized, disconnected, and refresh-failed states.
- Preserve the last complete usable projection during a failed refresh while
  visibly reporting its age and failure. Never combine partial pages or
  identities into a fabricated cluster view.
- Exercise the real worker route and serialized client path. A mock-only or
  direct-domain test does not close the increment.

### 2. Formation administration actions

- Add the accepted membership lock/unlock, join/leave, removal, blocklist, and
  tombstone actions only as their worker routes land.
- Require explicit, action-specific confirmation for destructive operations.
- Preserve operation IDs, exact-retry rules, acceptance receipts, unresolved
  outcomes, and recovery guidance. Successful transport is not convergence or
  command acceptance.
- Consume join material only from the accepted private-file workflow and never
  place its secret in widget state, logs, errors, command history, clipboard,
  or rendered buffers.

### 3. Workload and artifact operations

- Add workload compatibility, load/unload, run/start/step/stop/reset/resume,
  result, and checkpoint views/actions only after O-API-SHAPE/O-CLIENT and their
  runtime/storage authorities meet the promotion gate.
- Keep one-cluster/one-workload authority and committed-boundary semantics
  visible. Do not derive success from an entry node's local transient state.
- Keep bulk artifact download/export CLI-first unless an independently bounded
  interactive workflow is justified.

### 4. Events, logs, and live observation

- Add bounded event/log views after their retained/query or subscription
  contracts are accepted; keep operational telemetry distinct from scientific
  observation.
- Add live scientific observation only through S-OBSERVE/V-LIVE contracts.
  Observer overload must never affect scientific state or block simulation
  commit.
- Make baseline identity, completeness, staleness, reconnect, dropped updates,
  and unsupported capabilities visible rather than silently filling gaps.

### 5. Close operator parity

- Maintain a parity matrix mapping every supported administrative `orishuctl`
  command to its TUI view/action, public request/outcome contract, access tier,
  confirmation rule, operation/retry identity, audit behavior, and tests.
- The only default exception is raw formation-admission secret retrieval/export.
  Record any additional exception as an explicit product decision, not an
  omitted screen.
- Test representative reads, mutations, failures, retries, authorization
  denials, unavailable routes, and reconnects through real worker/client wire
  paths. Keep pure update and headless rendering coverage for all UI states.
- Reconcile the monitor manual, operator stories, roadmap, installation/release
  claims, and package metadata with the behavior actually delivered.

## Cross-cutting constraints

- The worker/cluster remains authoritative. TUI state is an independently
  refreshable presentation projection and never mutates domain state directly.
- Share public client request/projection types and semantics with `orishuctl`.
  Introduce shared CLI/config helpers only after two real consumers demonstrate
  the common behavior; do not make one application depend on the other.
- Bound response bytes, pages, records, refresh work, retained history, queues,
  diagnostics, and rendered rows before accepting hostile input.
- A slow terminal or observer may skip supersedable presentation refreshes. It
  must not coalesce correctness-bearing commands, receipts, recovery evidence,
  or scientific baselines.
- Secrets never enter logs, telemetry, persisted presentation state, command
  history, panic output, or screenshots by default.

## Acceptance criteria

This backlog task is complete only when:

- the parity matrix covers every supported `orishuctl` administrative command
  with a tested TUI equivalent or the documented admission-secret exception;
- all TUI operations preserve the public client's security, domain-outcome,
  retry, audit, and bounded-transfer semantics;
- unavailable or stale state is distinguishable from empty, healthy, or
  successfully committed state;
- real serialized worker/client tests cover each promoted API family, while
  pure update and headless rendering tests cover its UI states; and
- documentation and release claims describe only implemented behavior.

Each promoted increment must also define and pass its proportionate
package/workspace checks and manual terminal verification before it is marked
complete.

## Non-goals

- Replacing `orishuctl` for scripts, automation, or evidence-preserving
  private-file workflows.
- Designing monitor-only worker routes, domain models, authentication, retries,
  or mutation semantics.
- Revealing raw formation-admission secret material in the TUI.
- Editing experiments or rendering scientific scenes; those belong to Kagami.
- Replacing Prometheus/OTLP operational telemetry, dashboards, or alerting;
  those belong to P-OBSERVABILITY/P-OBS-DOCS.
- Treating every planned increment as one review or implementation change. Each
  increment must be refined and assigned independently when its gates hold.

## Dependencies

- The predecessor TUI shell.
- N-FORMATION and its accepted membership projection/client route for the first
  live increment.
- O-API-SHAPE and O-CLIENT for the general operator resource surface.
- O-RUNTIME, O-STORAGE, S-OBSERVE/V-LIVE, and later distributed authorities for
  the views and actions that consume their state.
