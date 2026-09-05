# `orishu-membership` benchmarks

Criterion microbenchmarks for the sans-IO transition core, following the same
split as [`orishu-variables`](../../orishu-variables/benches/README.md):
wall-clock throughput here, and nothing else — Criterion has no notion of
allocations, and this crate has no allocator-profiling example yet.

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

Seven groups:

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
  do not hide each other.
- `admission` — the full three-step handshake: request, verified credential,
  allocated identity. Expected to grow with formation size, because the
  acceptance carries a bootstrap membership snapshot; that clone is the point
  of the reply, not an accident.
- `gossip_queue` — selecting a piggyback batch from a queue of `depth` deltas,
  which sorts candidates on every send.

HTML reports land in `target/criterion/`.

### Sizes

| Env var | Default | Format | Applies to |
|---|---|---|---|
| `ORISHU_MEMBERSHIP_BENCH_MEMBERS` | `10,100,1000,10000` | comma-separated `usize`s | `merge`, `merge_batch`, `probe_cycle`, `digest`, `anti_entropy_collect`, `admission` |
| `ORISHU_MEMBERSHIP_BENCH_QUEUE_DEPTHS` | `16,128,512` | comma-separated `usize`s | `gossip_queue` |

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
consuming it, such as `MembershipTree::build`.
