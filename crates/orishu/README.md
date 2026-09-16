# orishu

The shared Orishu domain models and the client interface that applications use
to talk to a cluster. Persisted and protocol types currently live here. Orishu
is client-server from outside and peer-to-peer inside; a client entry node
relays the state a cluster collectively committed, and is not an independent
source of truth.

## Public contract

Two modules make up the surface consumers use:

### `model` — persisted and protocol types

The typed domain: `cluster`, `node`, `workload`, `manifest`, `run`, `run_load`,
`checkpoint`, `result`, `audit`, `blocklist`, `tombstones`, `storage`, and
`quantity`. These are the persisted and wire shapes shared across
`orishuctl`, `orishu-monitor`, and `orishu-worker`. They reuse the shared
identity types from `orishu-identity` and the resource envelope from
`orishu-resource`.

### `client` — the cluster client interface

A set of capability traits over one HTTP transport:

| Trait | Surface |
| --- | --- |
| `ClientApi` | The composed client. |
| `ClusterApi`, `MembershipApi` | Cluster and membership queries. |
| `WorkloadApi` | Workload submission and lookup. |
| `ResultsApi`, `CheckpointApi` | Committed results and checkpoints. |
| `BlocklistApi`, `GraveyardApi` | Blocklist and tombstone administration. |

Supporting types include `Credentials`, `ClientError`, `ApiRoute`,
`ApiResponse`, `PaginationCursor`, and the address/auth/CBOR middleware under
`client::address` and `client::http_client`. `library_version()` reports the
crate version.

## Dependency note

This crate links `reqwest`, `tokio`, and `chrono`. The shared identity contract
lives in the dependency-poor `orishu-identity` crate precisely so the
`orishu-membership` sans-IO core can use the same identities without inheriting
this client's dependency tree.

## Testing and fuzzing

Contract testing: **has.** `tests/resource_wire.rs` pins the serialized
resource-envelope contract, and `tests/run_identity.rs` / `tests/run_load.rs`
cover run identity and load parsing.

Fuzzing: **needs (covered indirectly).** The client decodes responses from a
cluster entry node, which the model treats as untrusted. The actual hostile
network bytes for the peer and worker paths are fuzzed through the
`orishu-worker` targets in [`fuzz/`](../../fuzz), which depend on this crate.
A dedicated response-decode target would exercise the client-facing wire path
directly.
