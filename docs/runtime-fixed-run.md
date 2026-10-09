# Fixed-profile scientific run owner

Status: **implemented single-partition owner and bounded field/object-snapshot leases;
selected workload admission implemented; application integration remains work**.

`orishu_runtime::FixedRun` owns committed numeric objects, every selected field,
integrator history, simulation time and computed forces under
`orishu.force-then-integrate/v1`. It uses the existing real Component host, not
native reference physics. This implements the state-ownership/atomicity decision
in [ADR 0024](adr/0024-orishu-orchestrates-a-workload-component-graph.md); it is not
a second workload manifest, scheduler or cluster authority.

## Input and lifecycle

`RunProgram` is an in-memory execution projection: a canonical complete field set,
one integrator, exact instance contexts, captured configuration/domain inputs,
coupling packets, declared state extents and authored fixed timestep. The shared
`orishu_runtime::admit` path constructs it after verifying the
immutable root/closure, model-family exclusivity and graph/profile compatibility;
see [workload v3](workload-v3.md). Its public raw constructor validates structural relationships, exact input
identities, packet membership/roles, extents and each kernel's numerical inputs;
it **does not establish release-closure provenance**.

`from_captured` loads and validates supplied field state and history. It never
calls field/history initialization, substitutes defaults or resolves providers.
The whole-object packet includes uncoupled static objects; field and Dynamics
projections derive from that common state. The initial boundary is zero at time
zero, with no computed force batch (not invented zero-valued observations).

An explicit synchronous `advance` performs:

1. Derive all coupling projections from the same committed object state. Keep
   captured source/response strengths and refresh only intrinsic kinematics.
2. Invoke every field against its own prior state, retaining all candidates privately.
3. Validate complete force coverage and reduce by ascending field-instance ID.
4. Invoke the selected integrator once. Its output may change kinematics of the
   existing Dynamics set, never membership, inertial mass or static objects.
5. Validate every candidate field and the integrator history against the resulting
   object projections and authored timestep. Check cancellation before publication.
6. Ask the embedding publication authority to accept the whole candidate, when
   using `advance_with_commit`. Plain `advance` uses an always-accepting gate.
7. Replace the complete committed boundary in one non-fallible assignment.

The gate is trusted host authority, not an observer or guest hook. Refusal preserves
the previous state; acceptance is final, with no later cancellation/fallible check
that could roll the executor back behind an externally published boundary. It must
not publish unaccepted candidates. The worker's retained executor uses this gate
to serialize boundary publication with its formation owner; it terminates on lost
coordination rather than retrying with uncertain private/public state. See
[ADR 0028](adr/0028-fence-worker-scientific-admission-through-formation-owner.md).

No field reads another field's candidate. Computed forces attached to boundary
N+1 were evaluated at N's entity kinematics; field-state phase semantics remain
model-declared. No fields yields a complete zero-force Dynamics batch and ordinary
inertial motion. No dynamic entities is an explicit empty set.

Failures in any phase leave all previous scientific bytes, time and boundary
unchanged. Attempt IDs advance independently and are not simulation time. Typed
kernel errors survive under host `RunFailure` attribution for the affected
instance/stage/run/epoch/boundary. Scheduling, background execution and external
command transport are the embedding application's responsibility. `stop` is
terminal for that owner but retains committed state for inspection/checkpointing.
The worker embedding's `step_at`/`stop_at` additionally compare an explicit expected
boundary inside its serialized operation slot. This is a command precondition,
not part of the numerical kernel interface or an operation receipt; stale retries
do not advance again. Public pause/resume and step-budget scheduling must not be
implemented by calling this terminal stop and later pretending it can resume.

## Complete checkpoints and observations

`checkpoint` invokes all selected checkpoint exports against one committed boundary,
validates the complete portable result, and returns all parts or no checkpoint.
`restore` loads every captured field/history part into freshly compiled compatible
instances, validates them and preserves exact continuation. It does not initialize
natural defaults. The run/epoch, contexts, captured coupling definitions, declared
extents and timestep must match. A model switch is not a checkpoint relabeling.

`RunCheckpoint` is currently an in-memory aggregate of portable buffers and exact
compatibility metadata, **not a new durable file or protocol schema**. Storage
record/closure validation and epoch-changing cluster recovery still need their
reviewed adapters. `RunScope` names the workload root and a run-descriptor content
reference. The latter must be tied to the existing protocol's formation/workload/
epoch `RunIdentity`; this owner neither allocates cluster run IDs nor changes that
protocol. Low-level owner tests use synthetic descriptors/provider pins; the
separate admission suite verifies real release evidence and selected closures.
The worker adapter now obtains that scope from its formation owner's
[canonical run-descriptor allocation](run-descriptor-v1.md), rather than accepting
a submitter-supplied descriptor. This does not add formation dependencies to the
shared numerical runtime or implement distributed/reset allocation.

`acquire_field` returns a separate `FieldSnapshot`, without borrowing the run for
the duration of a query. Its exact committed state/context/source remain fixed as
the owner advances. Sampling executes in an isolated disposable guest; a stale
query source, malformed request, trap or cancellation does not alter run state.
One lease permits at most one simultaneous sampling invocation.

`acquire_objects` returns an `ObjectSnapshot`: the whole canonical object packet
and optional reduced-force packet from one committed boundary, sharing immutable
buffers without a numerical copy or guest invocation. The source names workload,
run descriptor, epoch, boundary and SI simulation time. Initial force absence
remains None rather than invented zeroes; later forces were evaluated at the
predecessor's kinematics. This is not an observer wire schema or checkpoint.

`ObjectSnapshot::encode` now wraps those packets in the shared bounded
[object-observation v1 payload](object-observation-v1.md). It scans/copies retained
bytes into reusable caller storage and returns a whole-frame digest, so adapters
must call it off the scientific executor, retaining the lease. Shared readers
validate identity, complete dynamic-force coverage and all numeric records before
exposing borrowed objects. The payload excludes fields and is not a subscription,
baseline protocol or remote delivery implementation.

Default observer policy is eight leases and 128 MiB shared across retained field,
object and force bytes. Object acquisition reserves the combined packet sizes
before sharing them; old leases remain valid through advancement and disposal.
Object acquisition is O(1) in numerical packet bytes: it counts extents and shares
immutable buffer handles rather than scanning objects or forces. Cold schema/source
metadata is still cloned; this is not an allocation-free observation claim.
Acquisition is non-waiting and rejects excess; scientific commit never acquires the
observer-budget mutex. Duplicate leases conservatively charge the full state size.
Dropping a lease returns its quota. State digests are computed before publication,
so acquiring a field lease does not rescan large field bytes. Object leases do not
hash their packets during acquisition. Other cold metadata still
allocates. UI/MCP routing, per-observer policies, sampled-value caches and recording
adapters remain work. Host callers retaining arbitrary checkpoint/state clones are
not untrusted observer admission; those adapters must impose their own quotas.

The worker now acquires observations through a separate eight-request bounded
ingress instead of its scientific command slot. It checks command ingress before
each observer request, never waits for observer consumers and performs no sampling
guest invocation on the acquisition lane. Idle acquisition can wait up to the
existing 50-ms executor poll; continuous commands may starve observations rather
than await them. Exact-boundary `acquire_field_at`/`acquire_objects_at` validate
against committed state inside the executor. A stale request refuses rather than
combining boundaries. Queued observations are discarded on revocation/publication
loss; field sampling remains detached after a lease is acquired. The
[bounded object-read HTTP adapter](protocol-object-observation-v1.md) now uses
this ingress and off-executor encoding. Field queries and resumable observation
streaming remain unwired; the one-shot object read is not a subscription.

## Bounds, evidence and remaining delivery

Owner limits cover field/object/aggregate coupling counts, declared scientific
packet extents and cold input bytes; sandbox/grant and sampling limits also apply.
The scientific-packet accounting is **not** a complete resident-memory bound:
temporary typed projection vectors, validation copies, guest memories, JIT code,
concurrent operations and externally retained checkpoint/state clones still need
the aggregate arena/store budget. Input byte excess is refused before hashing.

Projection construction costs O(objects + coupled records × log(objects)); stable
reduction uses the documented [bulk IO bounds](scientific-bulk-io.md). Guest model
cost is additional and independently metered. This initial owner still creates
disposable stores and candidate/validation buffers per operation; it is a correct
baseline, not a production hot-path or zero-allocation claim.

`tests/fixed_run.rs` executes the real Newtonian/Euler Components and proves:

- simultaneous whole-state commits, refreshed common-boundary projections and
  independent inertial versus source/response masses;
- unchanged static/uncoupled objects, empty dynamic sets and no-field drift;
- failures in a later field, Dynamics and final validation preserve every prior part;
- complete checkpoint/fresh-engine restore yields the same next state;
- old snapshot sampling overlaps subsequent advancement, and observer quota,
  source mismatch or cancellation does not block commit;
- malformed coverage, coupling state, history and aggregate-limit refusals.
- external commit-gate refusal preserves prior buffers; acceptance remains final
  even if the operation is cancelled immediately afterward.

Internal production-helper tests additionally reject integrator outputs that add,
drop or relabel entities, write static objects or change inertial mass.

Remaining full-goal work includes reusable stores/arenas and aggregate/JIT limits,
adaptive step/checkpoint state sizing, runtime membership/emitter scheduling,
guarded authoring effects/window export, durable checkpoint and observation
adapters, and public Kagami/worker run integration. Captured v4 files and headless
portable export already reach real admission, and the internal retained worker
executor now uses the commit gate. FixedRun currently has a
fixed membership and declared fixed state extent; it must not silently accept a
workload requiring the unimplemented lifecycle transitions or distributed exchange.
Natural field/history initialization now supports explicit bounded output grants;
this removes layout knowledge from initial capture but does not change the owner's
fixed step-state extent policy.
