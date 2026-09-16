# Immutable run descriptor v1

Status: **shared model/codec and single-node worker allocation implemented;
opt-in HTTP load/descriptor retrieval implemented; public run control, references
and distributed allocation remain work**.

A workload describes a simulation problem. Executing that same workload twice
must yield different run identities without changing the workload digest. The
shared `orishu::model::run` types implement the existing protocol tuple and its
immutable descriptor, following [ADR 0029](adr/0029-bind-run-descriptors-to-owner-allocated-epochs.md).

## Shape and identity

Human-readable projection:

```json
{
  "apiVersion": "orishu.run-descriptor/v1",
  "run": {
    "formationId": "formation-a",
    "workloadId": "sha256:0000000000000000000000000000000000000000000000000000000000000000",
    "workloadEpoch": 1
  }
}
```

The all-zero digest is illustrative, not an executable workload. The scientific
profile uses a real verified canonical workload root. FormationId uses the shared
validated identity type; WorkloadEpoch is uint64 (the allocator starts at one).
No node identity, label, URL, credential, wall time, cursor or scientific state
belongs in these bytes. This is a runtime-produced immutable fact record, not a
generic authored resource or mutable status resource. It is not injected into the
submitted workload or its exported closure.

Descriptor identity is SHA-256 over explicit deterministic CBOR, using the existing
workload canonical codec. Root keys sort `run`, then `apiVersion`; nested keys sort
`workloadId`, `formationId`, `workloadEpoch`. Lengths and integers use their shortest
definite encoding. Unknown/duplicate fields, indefinite lengths, tags, trailing
data and noncanonical spellings are refused. JSON/serde formatting is not identity.
The example's canonical bytes have digest
`sha256:32fe2f909357a90b8e27384bd128e81b1f664265f0dd2931f7e254b2d2d46d53`.

Both input paths bound bytes at 512 before parsing. Canonical decoding additionally
uses depth three and a 160-unit reader budget (the shared decoder also checks text
extents against remaining budget); the typed record requires exactly eleven tree
nodes and no collections beyond the two fixed maps. Tests independently pin bytes
and digest, check JSON/CBOR interoperability, maximal identities/epochs and hostile
encodings. Generic serde callers must still bound their outer transport frame.

## Worker allocation and use

The locked single-node formation owner allocates a checked epoch only after complete
portable-closure verification and denial checks, before actual Component admission.
An allocation is immutable for its execution lease/root. Identical internal requests
recover the same descriptor; changing the root or lease is refused. Epochs increase
independently of reservation sequences and are never reused after failure/unload.
Exhaustion refuses new allocation. Restart creates a new formation, so reused labels
or credential files cannot resurrect a prior run identity.

Preparation reserves four control slots initially: reservation request, release,
allocation and confirmation. Three remain after reservation and two after allocation.
The worker's public Rust admission method accepts portable bytes, not a caller-chosen
RunScope. It supplies the assigned scope to shared runtime admission; confirmation
and every boundary publication check that exact allocation. Candidate, validated
admission and retained run handles expose the immutable descriptor. Bounded field
leases use its digest and epoch in their committed observation source.

`RunningWorker::scientific_admission` binds this path to real process bootstrap;
`prepare_for` retains an explicit expected-formation precondition. This is an
internal IO-adapter seam, not a new HTTP route or execution-capability advertisement.
An internal [durable identified load journal](run-load-receipts-v1.md) now retains
request/descriptor facts and conservative retry history, without restoring runs.
The daemon coordinator now retains admission and exact-identity runs independently
of response futures. Public authorization/delivery, automatic startup and descriptor retrieval,
shareable RunReference hints, checkpoint-created epochs and multi-node allocation
remain separate work. Shared runtime code remains formation independent; local
hosts supply their execution scope through their own authority.
