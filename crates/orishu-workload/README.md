# orishu-workload

The immutable Orishu workload: its manifest, the content-addressed artifact
closure it depends on, and the canonical codec that gives it identity. Kagami
compiles an experiment into these types and Orishu admits them. There is no
second application-specific schema and no translation between two models that
could disagree, so this crate sits below both and stays dependency-light. See
[workloads](../../docs/workloads.md).

## The shape of a workload

```text
workload = manifest + the transitive closure of the artifacts it names
```

A `WorkloadManifest` declares a bounded graph of component instances, the typed
channels between them, and a deterministic step plan ([ADR 0024]). Every
executable and input is named by an `ArtifactDescriptor` — a role, a content
`ArtifactDigest`, an exact size, and a media type ([ADR 0010]).

## Public contract

| Item | Role |
| --- | --- |
| `WorkloadManifest`, `WorkloadMeta`, graph and step-plan types | The immutable declaration. |
| `ArtifactDescriptor`, `ArtifactRole`, `SchemaCompat` | How a workload names each byte it needs. A descriptor has no URI, path, tag, peer, or credential: location is distribution, not identity. |
| `workload_digest` (`canonical`) | Hashes the canonical encoding of the root manifest. Because every dependency is named by digest, one value commits to the whole closure. |
| `manifest_from_canonical_bytes` (`canonical`) | Recovers a manifest from canonical bytes. The round trip proves the encoding is injective. |
| `BlobVerifier`, `VerifiedClosure` (`closure`) | Streaming closure verification. A candidate artifact arrives in chunks, is hashed as it goes, and is never materialised. |
| `authoring`, `domain`, `value`, `ids`, `digest` | Authoring bounds, domain/discretization specs, finite scalar values, identifiers, and digest algorithms. |
| `Limits` | The caller-owned bounds; permits a 64 GiB artifact honestly because nothing large is held. |

## Three things a workload deliberately cannot say

- **Where to get its bytes.** Location is distribution metadata; changing it
  does not produce a different workload.
- **What it is currently doing.** The manifest's status type is uninhabited, so
  a phase, epoch, or partition map is unrepresentable.
- **Which cluster it belongs to.** `WorkloadMeta` has no server-assigned uid or
  namespace, so an admission-time identifier cannot reach the digest.

JSON and YAML are how a person writes a workload; they never define its
identity, so whitespace, key order, comments, and codec choice cannot change it.

## Testing and fuzzing

Contract testing: **has.** `tests/canonical.rs` exercises the codec and digest
identity, `tests/closure.rs` the streaming verifier, `tests/authoring_bounds.rs`
the bounds, `tests/v3.rs` the v3 profile, and `tests/dependencies.rs` the
dependency budget.

Fuzzing: **has.** The `workload_canonical` target in [`fuzz/`](../../fuzz)
decodes untrusted bytes through `canonical::decode` and
`manifest_from_canonical_bytes` at the admission boundary, asserting codec
injectivity (decode, re-encode, decode) and a stable digest across a round trip.
Chunked `BlobVerifier` closure fuzzing remains a useful future addition.

## Benchmarks

```sh
cargo bench -p orishu-workload --bench workload
cargo run --release -p orishu-workload --example profile_workload --features dhat
```

The benchmark measures the identity path — canonical encode, `workload_digest`,
`manifest_from_canonical_bytes`, and `validate_closure` — over synthetic,
in-memory manifests, scaling with the component count (and, for the closure,
artifact bytes). Sizes are overridable via `ORISHU_WORKLOAD_BENCH_COMPONENTS`
and `ORISHU_WORKLOAD_BENCH_CLOSURE_BYTES`. The `dhat` example reports the
allocation cost of the same phases and confirms closure verification streams
rather than allocating per artifact byte.

[ADR 0010]: ../../docs/adr/0010-content-addressed-workload-closure-and-portable-bundles.md
[ADR 0024]: ../../docs/adr/0024-orishu-orchestrates-a-workload-component-graph.md
