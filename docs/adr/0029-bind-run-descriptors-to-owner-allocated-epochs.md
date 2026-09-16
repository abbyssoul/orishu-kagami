# ADR 0029: Bind run descriptors to owner-allocated workload epochs

Status: **accepted; shared descriptor and internal single-node owner allocation implemented**
Date: **2026-09-16**
Refines: [ADR 0013](0013-cluster-formation-and-node-identity.md),
[ADR 0028](0028-fence-worker-scientific-admission-through-formation-owner.md),
and the existing [RunIdentity protocol](../protocol-client.md#runidentity-and-runreference).

## Problem and options

The shared runtime requires a content-addressed run descriptor. Until this
refinement, its worker adapter accepted a host-supplied scope and its tests supplied
synthetic descriptors. A workload digest alone cannot distinguish two executions
of the same initial conditions. Routes, labels, plugin inventories and process-local
reservation sequences are not run identity either.

- **Keep accepting arbitrary caller-assigned scope.** Rejected for worker admission:
  a submitter could rebind provenance or reuse a prior execution identity.
- **Hash the workload and lease sequence directly.** Rejected: the lease sequence
  is an internal coordination mechanism, not the existing protocol identity, and
  omitting formation identity allows restart collisions.
- **Allocate the existing protocol tuple and hash an explicit descriptor.** Chosen:
  retain formation/workload/epoch semantics, make the descriptor independently
  readable, and reuse the shared bounded canonical codec.

## Decision

`orishu::model::run` owns `RunIdentity`, `WorkloadEpoch` and the immutable
`orishu.run-descriptor/v1` fact record. It is not an authored resource, a mutable
run-status resource or a second workload manifest. It carries only `apiVersion`
and `run: {formationId, workloadId, workloadEpoch}`. For the implemented scientific
profile, workloadId is the exact canonical workload root digest. SHA-256 over the
explicit deterministic CBOR representation is the descriptor artifact identity.
Labels, node IDs, routes, credentials, wall time and current simulation state are
excluded. Human-readable serde output is not the identity encoding.

Use the existing `orishu-workload` canonical value/codec with a 512-byte,
160-reader-unit, depth-three budget. The reader's budget also bounds individual
text extents; the typed shape permits exactly eleven key/value/container nodes.
A descriptor is a leaf fact record; it does not pull an artifact
closure from arbitrary URLs. Future checkpoint provenance or execution-fact fields
need an explicit format extension, not changes to existing descriptor bytes.
`orishu` may depend on the dependency-poor workload crate; the reverse remains
forbidden. The shared numerical runtime remains formation/protocol independent.

In the locked standalone first profile, the formation owner allocates a checked,
monotonically increasing epoch (first issued value one), separately from its lease
sequence. It does so only after a bounded complete portable closure has been
verified, before Component admission. The allocation is pinned to that lease and
root; repeated requests for the same lease/root return the same descriptor, while
rebinding to another root is refused. Failed/cancelled admissions burn issued
epochs. Release or unload cannot reuse them; exhaustion fails closed. Keeping the
counter increasing across formation changes is permitted, since formation identity
is also part of the tuple. Process restart creates a new formation and does not
restore this transient counter.

Owner confirmation checks the descriptor against the scope actually passed to
shared runtime admission. Boundary publication must continue to match that exact
allocated scope. Only the owner-issued descriptor reaches the worker executor;
the public worker preparation API no longer accepts a caller-assigned RunScope.
The shared local runtime still accepts a host-owned scope, as it does not allocate
formation identities or replace Kagami's local execution authority.

Allocation uses capacity reserved before admission and returns through a bounded
completion channel. Losing a reply cannot turn an allocated epoch into a reusable
one. This is not durable public command receipt handling or distributed allocation:
public submission/retry, multi-node epoch agreement and checkpoint-created epochs
still need their own reviewed execution/control adapters.
