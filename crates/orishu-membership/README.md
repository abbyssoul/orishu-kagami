# orishu-membership

The sans-IO functional core for cluster membership: one worker's view of who is
in its formation and whether each member is alive. It is an immutable-by-contract
`Membership` model, a closed `Message` enum of everything that can change it, and
a pure `update` that folds one message into the next model plus a bounded list
of `Effect`s.

## Public contract

| Item | Role |
| --- | --- |
| `update(model, message) -> Transition` | The one entry point. Pure: it never opens a socket, sleeps, reads a clock, generates randomness, or touches the filesystem. |
| `Membership` (`model`) | The immutable model of members and their state (live, suspected, departed). |
| `Message`, `Command` (`message`) | Everything that can change the model, including a local command such as `StartProbeRound`. |
| `Effect`, `EffectOutcome` (`effect`) | A request the shell must perform (select peers, mint a node ID, verify a token) and the correlated result it feeds back. `update` never treats an effect as having succeeded. |
| `Transition`, `Transition::foreign_gossip` | The next model, its effects, and non-membership gossip carried across untouched and never decoded here. |
| `GossipDelta`, `GossipQueue`, `ForeignDelta`, `OpaquePayload` (`gossip`) | The one deterministic merge path for every carrier. |
| `AdmissionBaseline` (`baseline`), `admission` | Every admission gate, decided atomically. |
| `antientropy` | Canonical hashing and bounded resumable rounds. |
| `Limits`, `LimitsSpec`, `LimitsError`, `DurationMillis` | The caller-owned bounds. |
| `testing` | Model builders for asserting on `(model, message) -> model` triples. |
| re-exports from `orishu-identity` | `FormationId`, `NodeId`, and the rest of the shared identity contract. |

## What sans-IO buys and costs

Every membership transition is a value. A recorded message sequence replays to
exactly the same state on a machine with no network, which makes the SWIM state
machine, the admission gates, and the merge rules testable without a runtime, a
fake socket, or a sleep. The cost is that the shell owns more: it must correlate
outcomes back to the requests that produced them, and it must not apply a
decision the core has not made. The crate must never acquire a networking,
async-runtime, clock, filesystem, TLS, or RNG dependency; `tests/dependencies.rs`
enforces that budget.

## What the core deliberately does not own

Wire framing, CBOR, QUIC, mTLS, address resolution, timer wheels, and entropy
belong to the IO shell. Non-membership gossip — `WorkloadUpdate`, checkpoint,
result, and audit deltas — is carried across untouched in
`Transition::foreign_gossip` and never decoded here.

## Testing and fuzzing

Contract testing: **has.** `tests/admission.rs`, `tests/convergence.rs`,
`tests/join.rs`, and `tests/swim.rs` assert on message-folding triples;
`tests/wire_fixtures.rs` pins the serialized type contract; `tests/dependencies.rs`
enforces the dependency budget.

Fuzzing: **has.** The `membership_messages` target in [`fuzz/`](../../fuzz)
exercises serde parsing of gossip, tombstones, Merkle digests, and admission
baselines, and round-trips successful JSON values. Because membership is sans-IO
and has no network codec, the actual hostile network bytes are fuzzed in the
`orishu-worker` targets (`worker_peer`, `worker_frames`, `worker_catchup`).
