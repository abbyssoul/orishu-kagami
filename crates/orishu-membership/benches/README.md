# `orishu-membership` benchmarks

Criterion microbenchmarks for the sans-IO transition core, following the same
split as [`orishu-variables`](../../orishu-variables/benches/README.md):
wall-clock throughput here. Criterion has no notion of allocations; the worker's
[formation allocation example](../../../apps/orishu-worker/benches/README.md)
also profiles the production gossip queue separately under DHAT.

## Why benchmark a crate that does no IO

Precisely *because* it does no IO. A worker's observed membership latency will
be dominated by CBOR encoding, QUIC, and the timer wheel, none of which live
here. That makes it easy for a regression in the pure part to hide behind
network noise until a large formation makes it visible. These groups measure
the part where a regression is a design problem, on formations far larger than
any integration test would build.

That is not hypothetical. The first run of `merge/one_delta` showed a cost
linear in formation size — 2.3 µs at 10 members, 138 µs at 10,000 — for an
operation that should be one `BTreeMap` lookup. The cause was
`Membership::alive_count()`, an `O(n)` scan called on every inbound message to
scale the `log2(N)` protocol parameters. Replacing it with
`Membership::scale_size()` on the hot path brought 10,000 members to 6.8 µs, a
95% reduction, and flattened the curve.

## Running: `cargo bench -p orishu-membership`

Ten groups:

- `merge` — folding one gossip delta into a formation of `count` members.
  Should stay close to flat: one map lookup, one comparison, one insert. A
  linear curve means something has started scanning the member map again.
- `merge_batch` — a full piggyback batch (`maxGossipPerMessage` deltas) in one
  message, the realistic per-message cost during steady-state gossip.
- `probe_cycle` — a complete correlated probe: start round, supply the selected
  peer, receive the `Ack`. Four `update` calls, measuring probe bookkeeping
  rather than formation size.
- `digest` — building the canonical anti-entropy tree and folding its root.
  Genuinely `O(n)`, one SHA-256 per replicated entity. This is the most
  expensive thing the core does, and the reason anti-entropy runs on a 30 s
  timer rather than per message.
- `anti_entropy_collect` — gathering one bounded reply batch from an already
  built tree, separated from `digest` so tree construction and traversal costs
  do not hide each other. `late_cursor` resumes at the last member and returns
  one record, exposing cursor seeking independently of full-batch copying.
- `admission` — the full three-step handshake: request, verified credential,
  allocated identity. Expected to grow with formation size, because the
  acceptance carries a bootstrap membership snapshot; that clone is the point
  of the reply, not an accident.
- `gossip_queue` — selecting a piggyback batch from a queue of `depth` deltas,
  including mixed hop priorities and retirement. Queue construction **and
  destruction** are outside the timed section (`iter_batched_ref`). Earlier
  versions timed destruction of the whole queue; regenerate baselines rather
  than interpreting that harness correction as a product speedup.
- `membership_parse` — real serde JSON parsing of member, foreign and tombstone
  messages plus malformed input. Actual bounded network CBOR is measured in the
  worker suite; JSON is the core's public type contract.
- `membership_reject` — unauthenticated peer rejection across formation sizes.
- `membership_steady` — repeated authenticated Pings with increasing sequence
  numbers against one retained model. Includes production message construction
  and effects, without a model clone between messages.

HTML reports land in `target/criterion/`.

The [2026-09-12 measurements](../../../docs/measurements/formation-performance-2026-09-12.md)
record retained anti-entropy improvements and a rejected gossip-selection
experiment, including large-queue regressions and allocation trade-offs.

### Sizes

| Env var | Default | Format | Applies to |
|---|---|---|---|
| `ORISHU_MEMBERSHIP_BENCH_MEMBERS` | `4,19,100,1000,10000` | comma-separated nonzero remote-peer counts | transitions, digest, anti-entropy, admission |
| `ORISHU_MEMBERSHIP_BENCH_QUEUE_DEPTHS` | `16,128,512,4096` | comma-separated queue depths up to 8192 | `gossip_queue` |

The fixture adds a local member: `4` and `19` mean the five-/twenty-worker
formations used in the Pi experiments. Other sizes retain larger scaling cases.
The direct-ACK case requires two remote peers. These synthetic CPU workloads
do not establish network convergence, authenticated API capacity or Pi timings.

```sh
ORISHU_MEMBERSHIP_BENCH_MEMBERS=50,5000 cargo bench -p orishu-membership
```

A malformed value panics rather than falling back silently. Every formation is
generated deterministically from `orishu_membership::testing` — no `rand`
dependency (the crate forbids one), and the same count always produces the same
model, so results are comparable across commits.

## Adding a new group

Follow the existing shape: a deterministic `formation(count)` helper, a
`group.throughput(...)` sized to what one iteration actually does, and
`group.bench_with_input` keyed by that size.

`update` consumes its model and returns a new one, so anything that drives a
transition needs `b.iter_batched` with a `model.clone()` in the setup closure —
otherwise the clone lands inside the timed region and swamps the measurement.
Use plain `b.iter(...)` only for operations that borrow the model without
consuming it, such as `MembershipTree::build`, or explicitly retain the returned
model across iterations as `membership_steady` does. Queues mutated in place use
`iter_batched_ref` to exclude their teardown.

Coverage is intentionally split: existing integration tests cover lost packets,
indirect probes, failure detection, admission/recovery and anti-entropy
continuations; fuzzing covers hostile parser mutations. Further microbenchmarks
for these paths should follow profiles. This suite does not yet quantify every
SWIM failure path, bounded baseline import or allocation hot spot.
