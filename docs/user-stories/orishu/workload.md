# User stories: workload management

These stories are written primarily from the cluster user persona: the researcher or scientific practitioner who wants to run simulations, observe progress, and study results. Administrators make that possible, but this document focuses on the workload-facing experience once a cluster is available.

`orishu` keeps this intentionally simple: one cluster runs one shared workload at a time. The stories here cover the lifecycle of that workload, including compatibility checks, loading, starting, stepping, stopping, resetting from checkpoint artifacts, resuming, and monitoring status for iterative experimentation and long-running computation.

A workload manifest is a resource that defines a simulation workload. The
manifest contains _metadata_ (name, author, tags) and a _spec_ describing the
simulation domain, discretization parameters, references to external resources
such as initial conditions, and a reference to the workload component that
implements the governing simulation logic. A workload also declares
_requirements_ — the full set of conditions a node must satisfy: hardware
capabilities (CPU, memory, accelerators), runtime lifecycle compatibility, and
an execution profile (numeric mode, reduction order, determinism constraints).
Not all nodes may satisfy every requirement, so the cluster tracks which nodes
are eligible to run the workload. See the
[workload contract](../../protocol-workload.md) for the package lifecycle and
sandbox boundary.

Loading a workload into the cluster replicates the manifest as cluster state and causes eligible workers to fetch, validate, and load the referenced workload artifacts they need. In normal operation, workers converge on the same manifest and trust policy, but referenced inputs, code, checkpoints, and catch-up state may be fetched separately rather than embedded inline in one payload.
In normal operation, a cluster user or administrator may need to load a new workload into an idle cluster, replace the currently running simulation with a new workload, or gracefully stop a simulation and later resume it from a checkpoint artifact. They also need visibility into the current workload and simulation status, including which workload is active, how far along the simulation is, and how the domain is partitioned across nodes.

Workload bundles are produced by clients such as Kagami (File > Export...; see
[Export an experiment as a portable bundle](../kagami/authoring.md#export-an-experiment-as-a-portable-bundle))
or other workload tooling; `orishuctl` consumes them as one of several
distribution inputs.

To support simplified deployments, `orishu` offers a simple "fire-and-forget" command to run a simulation, as well as more granular commands to control each stage: load a workload, start a simulation, and stop it.

## Shared assumptions

Unless a story says otherwise, all stories in this document assume the following:

- A target cluster is already running and reachable. Cluster setup and node administration are covered in [worker-admin.md](./worker-admin.md) and [cluster-admin.md](./cluster-admin.md).
- The cluster is healthy enough for the requested workload operation unless the story explicitly describes degraded or failing conditions.
- The cluster user can reach the relevant local or remote client endpoint.
- Any referenced workload manifests, URLs, local paths, checkpoint IDs, or result IDs exist and are accessible unless the story is specifically about error handling.
- Authentication requirements depend on the command and are called out by each story where they matter.


## Workload lifecycle

### Run a simulation from a manifest

As a cluster user, I want to "just run" my simulation by providing a workload manifest so that I can quickly execute a defined simulation without needing to issue multiple commands.

**Given** a healthy cluster and a workload manifest
**When** I run the workload
**Then** the simulation is loaded from the manifest file and starts evolving across the cluster.

**Acceptance criteria:**
- A command is available to run a simulation from a supported distribution
  input, for example `orishuctl run <manifest-or-bundle>`. The command resolves
  the immutable root manifest and complete workload closure, waits for the load
  operation to prepare eligible nodes, and then starts the simulation.
- If the manifest cannot be fetched or parsed, the command fails with a clear error before any workload is stopped, replaced, or otherwise changes cluster state.
- Functional equivalence to running `orishuctl workload load <url/path>` followed by `orishuctl workload start`, after ensuring the load operation completes and all nodes report ready before starting.
- **Loading into an idle cluster:** (same as #Load a workload) The workload manifest is replicated through the cluster. Eligible nodes fetch the referenced workload artifacts they need, validate them, and transition to a `Ready` state. The cluster waits for all eligible nodes to report ready before starting the simulation.
- **Loading while a simulation is already running:** (same as #Load a workload) This follows the same replacement behavior as `orishuctl workload load <url/path>`: it is equivalent to gracefully stopping the current simulation followed by loading the new workload into an idle cluster.
- Running from a manifest is a privileged operation requiring authentication, as it may replace the currently active workload and starts execution.


### Load a workload

As a cluster user, I want to load a workload manifest into the cluster and prepare it for execution so that I can review the workload status and distribution across node population before starting the simulation.

**Given** a healthy cluster and a workload manifest
**When** I load the workload
**Then** the workload is loaded across the cluster and prepared for execution.

**Acceptance criteria:**
- A command is available to load a workload, e.g. `orishuctl workload load <url/path>`. The command submits a workload manifest to the cluster and causes eligible nodes to fetch, validate, and load the referenced workload artifacts they require.
- Gracefully stops any currently running simulation, if present, before loading the new workload. The cluster user does not need to issue a separate stop command — the load operation implies that. If the cluster user needs to bypass the graceful stop (e.g. the current simulation is misbehaving), they should explicitly run `orishuctl workload stop --force` before loading the new workload.
- Eligible workers converge on the same workload manifest and trust policy. Each eligible worker fetches and validates the referenced workload artifacts it needs, loads the simulation code, and prepares to compute on its assigned portion of the domain.
- Thin submission, portable-bundle import, cache reuse, and peer retrieval are
  distribution choices. If they resolve to the same root and artifact digests,
  workers treat them as the same workload and request only missing blobs.
- Artifact descriptors contain identity and compatibility only. Source URLs,
  cache paths, peer holders, and credentials remain outside the replicated
  workload definition and may change without changing its identity.
- Before a node reports `Ready`, it verifies workload content hashes, validates any required signatures, and confirms it can satisfy the workload's declared requirements (hardware capabilities, runtime lifecycle compatibility, and execution profile).
- Before instantiation, every node treats the package as hostile code and
  verifies the `wasm-component` engine, lifecycle and component world, closed
  import set, resource limits, and interruptibility. A valid signature never
  grants filesystem, network, clock, randomness, process, or thread authority.
- A guest trap, timeout, limit violation, invalid return value, or cancellation
  cannot commit partial state or corrupt the worker's cluster/runtime state.
- If the workload contains variables or expressions, Orishu validates their
  declared language version, bounds, names and field paths, dependency graph,
  dimensions, and finite results before any node reports `Ready`. Every
  participating node agrees on the canonical resolved-parameter fingerprint.
- The accepted source-bearing expressions and resolved parameter set are frozen
  for the workload epoch. Changing an expression requires normal workload
  replacement and never changes a running simulation in place.
- **Loading into an idle cluster:** The workload manifest is replicated through the cluster. Eligible nodes fetch the referenced workload artifacts they need, validate them, and transition to a `Ready` state. The cluster waits for all eligible nodes to report ready before the cluster user can issue a start command.
- **Loading while a simulation is already running:** This is equivalent to gracefully stopping the current simulation followed by loading the new workload. The currently running simulation is stopped, workers complete their current time step, and the runtime should attempt to write both a checkpoint artifact and a result artifact whenever storage and policy permit. Either artifact may still be incomplete if some required chunks were never durably written. The prior workload is then unloaded and the new workload is loaded. The cluster user does not need to issue a separate stop command — the load operation handles the transition gracefully. The cluster user receives feedback indicating that the previous simulation was stopped and the new workload is being loaded. If the cluster user needs to bypass the graceful stop (e.g. the current simulation is misbehaving), they should explicitly run `orishuctl workload stop --force` before loading the new workload.
- Loading is a privileged operation requiring authentication, as it modifies the cluster's active workload state.


### Unload a workload

As a cluster user, I want to unload a workload manifest from the cluster.

**Given** a healthy cluster with a workload loaded
**When** I unload the workload
**Then** the workload is removed from the cluster.

**Acceptance criteria:**
- A command is available to unload a workload, e.g. `orishuctl workload unload [--force]`. The command removes the currently active workload from live cluster state on all nodes in the cluster.
- **Unloading an idle cluster:** It is a no-op to unload a simulation if nothing is loaded. If no simulation is currently loaded, the command returns a message indicating there is nothing to unload.
- **Unloading while a simulation is already running:** By default, the command requests a coordinated unload using the cluster's normal transition path. With `--force`, the command requests immediate unload without waiting for coordinated shutdown or a consistent snapshot. In both cases the workload manifest is removed from all nodes once the unload is applied.
- Unloading a workload does not itself delete stored checkpoint artifacts or result artifacts.
- Unloading **ends the current result sequence**: a later load (of the same or a different workload) begins a new result sequence. The previously sealed stored result artifacts remain listable and downloadable; ending the sequence is a logical/operator-facing change, not a deletion or mutation.
- Unloading is a privileged operation requiring authentication, as it modifies the cluster's active workload state.


### Check workload compatibility

As a cluster user, I want to verify whether a workload manifest is compatible with the current cluster before loading it, so that I can identify issues (unsupported hardware, missing accelerators, ABI mismatches, unsigned artifacts) without disrupting a running simulation.

**Given** a healthy cluster and a workload manifest (URL or local path)
**When** I check workload compatibility
**Then** I receive a per-node eligibility report without any side effects on the cluster.

**Acceptance criteria:**
- A command is available to check workload compatibility, e.g. `orishuctl workload check <url/path>`. The command evaluates the referenced workload manifest against the cluster as a whole and reports if the manifest is valid and which nodes can satisfy the workload's requirements.
- The check covers at minimum: the workload's `requirements` — hardware capabilities (`requirements.hardware`), runtime lifecycle compatibility (`requirements.runtimeLifecycle`), and execution profile constraints (`requirements.executionProfile`) — as well as artifact signature validation when the cluster's trust policy requires signed workloads.
- The check evaluates the same bounded, versioned variables and expressions
  contract used by workload acceptance, including field references, cycles,
  dimensions, and canonical resolution, without loading or persisting it.
- The output includes:
  - Total number of nodes in the cluster versus number of eligible nodes.
  - For each ineligible node: the node ID and the specific reason(s) it cannot run the workload (e.g. "missing GPU", "unsupported runtime lifecycle: requires orishu.workload/v1", "unsigned artifact rejected by cluster policy").
  - A clear summary verdict: whether enough eligible nodes exist to run the workload.
- The command is read-only and does not load, distribute, or execute the workload. The manifest is fetched to the requesting node (or the contacted cluster node) for inspection but is not persisted or propagated to other nodes.
- No authentication is required for local access; remote access follows standard authentication requirements (same tier as other read-only operations).
- If the manifest cannot be fetched or parsed, the command fails with a clear error before attempting any node checks.


### Start a simulation

As a cluster user, I want to start the forward time evolution of a loaded simulation so that I can obtain results for analysis.

**Given** a loaded workload
**When** I start the simulation
**Then** the simulation begins evolving across the cluster from the initial conditions.

**Acceptance criteria:**
- A command is available to start a simulation, e.g. `orishuctl workload start`. The command tells the cluster to begin the forward time evolution of the currently loaded simulation when all eligible nodes are ready.
- If all eligible nodes are in the `Ready` state, computation begins simultaneously across all workers from the initial conditions.
- If some eligible nodes are still loading, computation begins when the last required node reports ready. The cluster user receives feedback indicating that the start command was received but is waiting for nodes to be ready, along with a breakdown of how many nodes are ready versus still loading.
- The run is associated with a new workload epoch, and partition ownership is fixed for that epoch until a safe rebalance boundary is reached.
- Starting a freshly loaded workload from initial conditions begins a **new result sequence**; result artifacts sealed during and after this run are segments of that sequence.
- If the simulation is already running, this command has no effect and simply confirms that the simulation is in progress.
- If no simulation is currently loaded, an appropriate error message is shown indicating that there is no simulation to start.
- Starting is a privileged operation requiring authentication.


#### Start with immediate execution

As a cluster user, I want to start the forward time evolution of a loaded simulation without waiting for all nodes to be ready, so that I can obtain results for analysis.

**Given** a loaded workload
**When** I force-start the simulation
**Then** the simulation begins evolving across the cluster from the initial conditions.

**Acceptance criteria:**
- A command is available to start a simulation with immediate execution, e.g. `orishuctl workload start --force` (or similar). In this mode, the cluster begins execution without waiting for all eligible nodes to reach `Ready`; each node begins computation as soon as it has loaded the workload and become eligible to contribute.
- Nodes that are slower to load effectively join a simulation that is already in progress. These late-joining nodes request checkpoint or catch-up state for the current workload epoch and committed simulation boundary, validate what they receive, and only then contribute to further computation, following the same mechanism used when a new worker joins an already-running simulation.
- Late-joining nodes do not begin computing from partial or guessed state. They must synchronize from validated checkpoint or catch-up state before they can own partitions for future steps.
- This mode trades coordinated start for faster time-to-first-result and is useful when the cluster has heterogeneous node performance or when minimizing idle time is preferred over a synchronized launch.
- If no simulation is currently loaded, the command fails with a clear error indicating that there is no workload to start.
- If the simulation is already running, this command has no effect and confirms that the simulation is already in progress.
- Immediate start is a privileged operation requiring authentication.


### Stop a simulation
As a cluster user, I want to stop a running simulation so that I can free up cluster resources, manage storage, or prepare for a new workload.

**Given** a simulation that is currently in the `Running` state
**When** I stop the simulation
**Then** the simulation halts forward time evolution across the cluster, and workers stop at a safe committed boundary.

**Acceptance criteria:**
- A command is available to stop a simulation, e.g. `orishuctl workload stop [--force]`.
- **Default (graceful):** Workers complete their current time step and the runtime should attempt to write both a resumable checkpoint artifact and a result artifact before stopping. This is the safe default that preserves data integrity, although either artifact may still be incomplete if some required chunks were never durably written.
- **With `--force`:** The simulation is stopped as quickly as possible. Workers halt computation at the earliest opportunity without waiting for a consistent snapshot. Use this when the simulation is misbehaving, producing incorrect results, or when preserving state is not needed.
- It is a no-op to stop a simulation that is not running. If no simulation is currently running, the command returns a message indicating there is nothing to stop.
- A graceful stop seals an **immutable** stored result artifact covering the simulation-time range computed since the last segment; it never edits a previously written artifact. That sealed artifact is a segment of the **current result sequence** for this loaded workload. If the operator later resumes the same stopped state (without rewinding to a different checkpoint), continued computation extends the *same* result sequence, and the next stop seals another segment.
- Stopping is a privileged operation requiring authentication, as it modifies the cluster's active workload state.


### Step a simulation

As a cluster user, I want to advance a simulation by a specific number of steps so that I can interactively inspect the simulation state, validate that the system is evolving correctly, and debug workloads incrementally — without committing to a full uninterrupted run.

**Given** a loaded workload in `Ready` or `Stopped` state
**When** I step the simulation by a chosen number of steps
**Then** the simulation advances by exactly that many steps and returns to `Stopped` state, optionally writing checkpoint artifacts if requested.

**Acceptance criteria:**
- A command is available to step a simulation, e.g. `orishuctl workload step [-n <count>] [--checkpoint-every <n>]`. The command advances the simulation by `<count>` time steps (default `1` if `-n` is omitted).
- After the requested steps complete, the simulation transitions to `Stopped` state automatically — the cluster user does not need to issue a separate stop command.
- **Default behavior:** Stepped execution does not automatically write checkpoint artifacts or result artifacts. This keeps interactive stepping lightweight.
- **With `--checkpoint-every <n>`:** The runtime writes a checkpoint artifact every `n` committed steps during the stepped run. This is useful when the cluster user wants recovery points during a longer interactive stepping session.
- While stepping, the simulation is fully `Running` — halo exchange, speculative execution, and all normal step coordination apply. The only difference from an open-ended run is that the runtime counts committed steps and stops after the requested count.
- The cluster user can observe intermediate state during stepping via `orishuctl workload stream` or by querying the workload status, just as during a normal run.
- If the simulation encounters an error during any of the requested steps, it transitions to `Error` state at that point. The remaining steps are not attempted.
- If the simulation is already `Running` (started via `orishuctl workload start`), the step command is rejected with an appropriate message — stepping is for user-controlled advancement, not for modifying an in-progress run.
- Stepping is a privileged operation requiring authentication.
- After stepping completes, `orishuctl workload status` reflects the updated simulation time and step count.
- Sequential step invocations accumulate: `step -n 3` followed by `step -n 2` is equivalent to `step -n 5` in terms of the final simulation state, assuming no errors and the same cluster topology.

**Example workflow:**

```
# Load a workload and inspect readiness
orishuctl workload load my-simulation.yaml
orishuctl workload status

# Advance one step, then inspect
orishuctl workload step
orishuctl workload status

# Advance 5 steps and checkpoint every step
orishuctl workload step -n 5 --checkpoint-every 1

# Satisfied — switch to continuous run
orishuctl workload start
```


### Reset simulation state to a checkpoint

As a cluster user, I want to reset a stopped simulation's state to a previously recorded checkpoint so that I can "rewind" to a known-good point and re-examine or re-run the simulation from there — without restarting from the original initial conditions.

**Given** a loaded workload in `Ready` or `Stopped` state and a valid checkpoint ID
**When** I reset the simulation state to that checkpoint
**Then** the simulation's current state is replaced with the checkpoint's state, and the simulation remains in `Stopped` state awaiting further user action.

This is the "rewind" control in the play/pause/stop/rewind analogy. It does not start computation — the cluster user must explicitly issue a start or step command afterward. This separation allows the cluster user to inspect the reset state, adjust parameters, or verify the checkpoint before committing to a new run.

**Acceptance criteria:**
- A command is available to reset simulation state to a checkpoint, e.g. `orishuctl workload reset <checkpoint-id>` (or similar). The simulation remains in `Stopped` state — no computation begins until the cluster user explicitly starts or steps the simulation.
- The checkpoint artifact must belong to the same workload definition (or a declared compatible successor) and must be complete enough to resume all required partitions. If the checkpoint is incomplete, incompatible, or does not exist, the command fails with a clear diagnostic rather than attempting a partial reset.
- Resetting creates a new workload epoch that records which checkpoint it originated from (visible in `orishuctl workload status` as the originating checkpoint).
- Rewinding to a checkpoint other than the current stopped state begins a **new result sequence** from that checkpoint baseline. Previously sealed stored result artifacts are left unchanged and remain individually listable and downloadable; the reset does not delete, rewrite, or mutate them.
- If the simulation is currently `Running`, the command is rejected with a clear message — the cluster user must stop the simulation first before resetting. Resetting a running simulation could lead to inconsistent state across nodes.
- After resetting, `orishuctl workload status` reflects the checkpoint's simulation time and the new epoch, confirming the state was rewound.
- The command is idempotent: resetting to the same checkpoint twice produces no error and does not create additional epochs beyond the first reset.
- Resetting is a privileged operation requiring authentication, as it modifies the simulation state.


### Resume from a checkpoint

As a cluster user, I want to resume a stopped simulation directly from a stored checkpoint so that I can continue useful computation without having to issue separate reset and start commands.

**Given** a loaded workload in `Ready` or `Stopped` state and a valid checkpoint ID
**When** I resume from that checkpoint
**Then** the simulation state is reset to the checkpoint and execution starts as one coordinated operation.

**Acceptance criteria:**
- A command is available to resume from a checkpoint, e.g. `orishuctl workload start --resume <checkpoint-id>` (or similar). The command resets the simulation to the specified checkpoint and starts it in a single step.
- The command is shorthand for `orishuctl workload reset <checkpoint-id>` followed by `orishuctl workload start`, and follows the same compatibility checks, completeness checks, and state-transition rules as those two operations.
- The checkpoint artifact must belong to the same workload definition (or a declared compatible successor) and must be complete enough to resume all required partitions. If the checkpoint is incomplete, incompatible, or missing required chunks, the command fails with a clear diagnostic rather than attempting a partial resume.
- If the simulation is currently `Running`, the command is rejected with a clear message rather than interrupting the in-progress run.
- Resuming creates a new workload epoch that records which checkpoint it originated from, and that epoch plus the originating checkpoint are visible in `orishuctl workload status`.
- Resume is a privileged operation requiring authentication.


## Monitoring and live observation

### Monitor simulation status

As a cluster user, I want to see the current status of the loaded workload and running simulation so that I can understand what is happening in the cluster and make informed decisions about running the simulation.

**Given** a cluster
**When** I request workload status
**Then** I receive a summary of the current workload metadata, simulation state, progress, and partition distribution across nodes.

**Acceptance criteria:**
- A command is available to monitor simulation status, e.g. `orishuctl workload status`. The output includes:
  - Workload loading status (e.g. not loaded, loading, ready, error)
  - Loaded workload metadata (name, author, tags).
  - Simulation status: 
    - `Loading`: workload artifacts are still being fetched, validated, or prepared,
    - `Running`: simulation in progress, 
    - `Stopping`: simulation is being stopped (checkpoint/result artifact writes or other coordinated shutdown work in progress),
    - `Stopped`: simulation paused and can be resumed,
    - `Ready`: workload is loaded and ready to start,
    - `Error`: simulation loading encountered an error and is not running.
  - Most recently recorded result artifact for the active workload epoch, if any
  - Current workload epoch and whether the run started from initial conditions or a checkpoint.
  - Current simulation time step and wall-clock elapsed time.
  - Node eligibility breakdown (how many nodes are eligible for computation out of total).
  - Node readiness breakdown (how many nodes are ready, loading, or computing).
  - Partition distribution across nodes (which nodes are computing which portions of the domain).
- If no workload is currently loaded, the command still succeeds and reports the unloaded/default state clearly (for example, `not loaded` with no workload metadata, no active epoch, and no partition assignments).
- The command is read-only and does not affect the running simulation.
- No authentication is required for local access; remote access follows the standard authentication requirements.


### Observe a live simulation

As a cluster user, I want to observe the current state of a loaded simulation in real-time so that I can monitor its progress, verify correctness, and diagnose issues as they occur — without having to wait for the simulation to complete or reconnect between runs.

**Given** a cluster with a loaded workload
**When** I connect to live simulation output
**Then** I receive a live stream of the simulation's current state, including its most recent computed data, and can continue receiving updates as the simulation progresses until I disconnect or the workload is unloaded.

This is analogous to joining a live video conference: the observer connects mid-stream and receives the current state immediately, then continues receiving incremental updates. Connecting does not affect the simulation or other observers.

**Acceptance criteria:**
- A read operation is available to connect to a loaded simulation and receive its current state snapshot followed by a stream of incremental updates while the simulation is running.
- If the simulation is currently in `Ready`, `Stopped`, or `Error`, the observer still receives the current snapshot and may remain connected waiting for the next state transition.
- If no workload is currently loaded, an appropriate response is returned indicating that no live output is available.
- Connecting to the live stream does not change simulation state or wait in
  the simulation commit path. Multiple concurrent observers are supported
  within enforced resource limits; overload rejects or sheds observation work
  rather than blocking simulation progress.
- Every observer owns an independent subscription and application cursor;
  changing one observer's channels, spatial region, level of detail, or
  live-follow state does not change another observer or the simulation.
- The stream includes a complete identified snapshot followed by deltas that
  name their base and target observations. A reconnect presents the newest
  observation it completely adopted; an unavailable or incompatible baseline
  recovers through another full snapshot.
- Disconnecting from the stream at any point is safe and does not affect the cluster or the result artifacts recorded by the cluster. The observer can reconnect later to continue receiving updates or to retrieve historical results.
- The operation is read-only. No authentication is required for local access; remote access follows the standard authentication requirements.
- If the simulation stops (gracefully or due to error) while an observer is connected, the observer receives an in-stream state or error indicator and may remain connected for a later restart.
- The stream is terminated only when the workload is unloaded or the observer disconnects.


## Results

### Replay persisted observations from a simulation time

As a cluster user, I want to read persisted observations beginning at a chosen
simulation time so that I can inspect or replay a run without downloading and
decoding every result artifact first.

**Given** complete result artifacts for a workload identity and epoch
**When** I request observations beginning at a simulation time
**Then** I receive a finite, ordered, exact stream beginning at the nearest
available committed boundary and continuing through the requested range.

**Acceptance criteria:**
- The request identifies the workload and workload epoch and may select a
  simulation-time start, end, channels, spatial region, and level of detail.
- The first frame is a complete observation at a reported committed boundary;
  subsequent frames are compatible, explicitly based deltas or complete
  observations.
- Historical delivery is reconstructed from immutable result artifacts and
  does not create a mutable result-sequence resource or alter those artifacts.
- Multiple clients can request different ranges and subscriptions concurrently
  without sharing a server-side playback cursor.
- Playback speed, pause, reverse navigation, and presentation interpolation are
  client concerns. The server returns exact stored observations in simulation
  order and never advances them according to wall-clock playback time.
- Pagination or continuation uses an opaque cursor tied to workload, epoch,
  subscription, and the last complete returned boundary.
- Missing, incomplete, corrupt, or unavailable coverage is reported with its
  exact range. The service never bridges a gap with fabricated or predicted
  scientific state.
- A client at the end of persisted coverage may explicitly switch to the live
  workload stream when the same workload epoch is still active. The server does
  not silently change modes or attach it to a replacement epoch.
- The operation is read-only and requires observation access, not workload
  submission or run-control capability.

### List historical result artifacts

As a cluster user, I want to see a list of all previously recorded result artifacts so that I can understand what has been computed, when, and with which workload definitions.

**Given** a cluster with recorded result artifacts
**When** I list results
**Then** I see a summary of all stored result artifacts, including metadata about each run.

**Acceptance criteria:**
- A command is available to list historical result artifacts, e.g. `orishuctl results ls [--workload-id <id>] [--workload-name <name>] [--after <timestamp>] [--before <timestamp>]`. The output lists stored result artifacts in reverse-chronological order by default.
- Each entry in the list includes a result identifier, the workload definition ID and workload name it corresponds to, the workload epoch that produced it, the time the result was recorded, the simulation time range covered, whether the stop was graceful, and the size of the stored artifact.
- The list can be filtered by workload definition ID, by exact workload name, and by time-based filters so a cluster user can narrow the output to a specific workload or recording window.
- The command is read-only. No authentication is required for local access; remote access follows the standard authentication requirements.
- If no results have been stored, an empty list is returned without error.


### Inspect a historical simulation result record

As a cluster user, I want to inspect the metadata of a previously recorded simulation result so that I can decide whether it is the artifact I want before downloading or deleting it.

**Given** a result artifact identified by its result ID
**When** I inspect the result metadata
**Then** I receive the stored result record metadata for inspection.

**Acceptance criteria:**
- A command is available to inspect a historical simulation result record, e.g. `orishuctl results get <result-id>`. The command returns metadata for the specified result record.
- The returned metadata includes enough detail to inspect the record before downloading, including the workload definition ID, workload name, workload epoch, and the content size of the stored result artifact.
- If the requested result ID does not exist, an appropriate error is returned.
- Inspecting metadata does not affect the stored artifact or any running simulation.
- The command is read-only. No authentication is required for local access; remote access follows the standard authentication requirements.


### Download a historical simulation result

As a cluster user, I want to download the output of a previously completed simulation run so that I can analyze it locally, archive it, or pass it to downstream tools.

**Given** a result artifact identified by its result ID
**When** I download the result
**Then** the result content is written to a local file.

**Acceptance criteria:**
- A command is available to download a historical simulation result, e.g. `orishuctl results get <result-id> --download -O <path>`. The command downloads the content of the specified result artifact to a local file.
- The cluster user must explicitly opt into content retrieval with `--download`; without that flag, the command returns metadata only.
- If the requested result ID does not exist, an appropriate error is returned.
- If required result chunks are currently unavailable, the command fails or clearly reports degraded availability rather than returning silently incomplete content.
- Downloading does not affect the stored artifact or any running simulation.
- The command is read-only. No authentication is required for local access; remote access follows the standard authentication requirements.


### Delete historical result artifacts

As a cluster user, I want to delete stored simulation result artifacts so that I can free up storage on cluster nodes and manage disk usage over time.

**Given** one or more stored result artifacts
**When** I delete one or more results
**Then** the specified artifacts are permanently removed from cluster storage and no longer appear in the results list.

**Acceptance criteria:**
- A command is available to delete a single historical simulation result, e.g. `orishuctl results rm <result-id>`.
- A command is available to bulk delete historical result artifacts, e.g. `orishuctl results purge [--workload-id <id>] [--workload-name <name>] [--after <timestamp>] [--before <timestamp>]`.
- Deleting a result is permanent and cannot be undone.
- After deletion, the artifact no longer appears in the results list and cannot be inspected or downloaded.
- Deleting historical results does not affect any currently running simulation, even if the running simulation was loaded from the same workload definition.
- The operation is idempotent: deleting a result that has already been deleted, or purging a set that is already empty, returns a clear outcome without breaking scripting or automation.
- Deletion is a privileged operation requiring authentication, as it is a destructive action that modifies cluster state.
- Each deletion produces an audit event recording which result was removed, when, and by which actor.
- Deletion is durable against node rejoin: a deleted result does not reappear if a node that was offline at deletion time later rejoins still holding local copies. The rejoining node's stale copies are reconciled away rather than re-added (purge tombstones; see the design doc). Deletion never silently re-admits a node or mutates another artifact.


## Checkpoints

### List stored checkpoints

As a cluster user, I want to see which checkpoints exist so that I can resume a simulation from a known-good state instead of restarting from scratch.

**Given** a cluster with stored checkpoints
**When** I list checkpoints
**Then** I see checkpoint metadata including workload ID, simulation time, resumability, and storage state.

**Acceptance criteria:**
- A command is available to list stored checkpoints, e.g. `orishuctl checkpoints ls [--workload-id <id>] [--workload-name <name>] [--after <timestamp>] [--before <timestamp>] [--resumable <bool>]`.
- Each entry includes checkpoint ID, workload definition ID, workload name, originating workload epoch, simulation time, creation time, and whether the checkpoint is resumable.
- The list can be filtered by workload definition ID, exact workload name, time-based filters, and resumability so a cluster user can quickly isolate the checkpoints relevant to a recovery workflow.
- Checkpoints that are incomplete are clearly marked as not resumable.
- The command is read-only. No authentication is required for local access; remote access follows the standard authentication requirements.


### Inspect checkpoint metadata

As a cluster user, I want to inspect the metadata of a checkpoint so that I can confirm it is the correct recovery point before resetting, downloading, or deleting it.

**Given** a checkpoint identified by its checkpoint ID
**When** I inspect the checkpoint metadata
**Then** I receive the stored checkpoint metadata for inspection.

**Acceptance criteria:**
- A command is available to inspect checkpoint metadata, e.g. `orishuctl checkpoints get <checkpoint-id>`. The command returns metadata for the specified checkpoint record.
- The returned metadata includes enough detail to inspect the checkpoint, including its workload ID, workload name, originating epoch, simulation time, and whether it is resumable.
- If the requested checkpoint ID does not exist, an appropriate error is returned.
- Inspecting checkpoint metadata does not affect the stored checkpoint, any running simulation, or the checkpoint's availability for resume operations.
- The command is read-only. No authentication is required for local access; remote access follows the standard authentication requirements.


### Download checkpoint data

As a cluster user, I want to download the data of a specific checkpoint to local storage so that I can archive it for future reference, transfer it to another cluster, or use it as initial conditions for a new workload — since checkpoint data uses the same format as initial conditions.

**Given** a checkpoint identified by its checkpoint ID
**When** I download the checkpoint
**Then** I receive the full checkpoint data written to a local file.

**Acceptance criteria:**
- A command is available to download checkpoint data, e.g. `orishuctl checkpoints get <checkpoint-id> --download -O <path>`. The command downloads the specified checkpoint data to a local file.
- If the exported checkpoint format is compatible with a workload's declared input expectations, the downloaded data may be reusable as initial conditions for a new workload manifest. This is distinct from the cluster's built-in reset/resume flow.
- The cluster user must explicitly opt into content retrieval with `--download`; without that flag, the command returns metadata only.
- If the requested checkpoint ID does not exist, an appropriate error is returned.
- If the checkpoint is incomplete or missing chunks, the command fails or clearly reports that the downloaded data may be incomplete so the cluster user can assess whether the artifact is usable.
- If the checkpoint metadata exists but required chunks are currently unavailable, the command fails or clearly reports degraded availability rather than implying the checkpoint is currently retrievable.
- Downloading does not affect the stored checkpoint, any running simulation, or the checkpoint's availability for resume operations.
- The command is read-only. No authentication is required for local access; remote access follows the standard authentication requirements.


### Delete stored checkpoints

As a cluster user, I want to delete stored checkpoints so that I can manage storage use and remove recovery points that are no longer needed.

**Given** one or more stored checkpoints
**When** I delete one or more checkpoints
**Then** the specified checkpoints are permanently removed from cluster storage and no longer appear in the checkpoint list.

**Acceptance criteria:**
- A command is available to delete a single stored checkpoint, e.g. `orishuctl checkpoints rm <checkpoint-id>`.
- A command is available to bulk delete stored checkpoints, e.g. `orishuctl checkpoints purge [--workload-id <id>] [--workload-name <name>] [--after <timestamp>] [--before <timestamp>] [--resumable <bool>]`.
- Deleting a checkpoint is permanent and cannot be undone.
- After deletion, the checkpoint no longer appears in the checkpoint list and cannot be inspected, downloaded, or used for reset/resume.
- Deleting stored checkpoints does not affect any currently running simulation beyond removing those recovery artifacts from future recovery workflows.
- The operation is idempotent: deleting a checkpoint that has already been deleted, or purging a set that is already empty, returns a clear outcome without breaking scripting or automation.
- Deletion is a privileged operation requiring authentication, as it is a destructive action that modifies cluster state.
- Each deletion produces an audit event recording which checkpoint was removed, when, and by which actor.
