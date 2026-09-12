# Local membership and worker performance — 2026-09-12

Three benchmark-guided experiments produced **two retained optimizations**:
anti-entropy reply collection and worker datagram trimming. A gossip-selection
candidate was rejected because its small-queue gains did not hold across the
large mixed-priority workload. No wire, persisted format, dependency, authority,
executor policy or host configuration changes are required.

## Baseline and method

The baseline is the incoming **dirty worktree at `c288c0a`**, including its
existing queue-selection, CBOR preflight and single-buffer framing improvements.
These results do not attribute those earlier changes to this round. Existing
unrelated work was preserved; the rejected gossip candidate was restored exactly
to its incoming source.

- Intel Core i7-1370P, x86-64 Linux 6.17.0-41-generic, 20 available logical CPUs.
- Rust 1.97.1, LLVM 22.1.6; Cargo's optimized benchmark profile, default features.
- Initial sweep: 20 samples, 0.5 s warmup, 1 s measurement per case.
- Confirmation: separately frozen before/after executables, 30 samples,
  1 s warmup and 2 s measurement. No concurrent builds, tests or fuzzing during
  Criterion timing. DHAT allocation counts were collected separately; its
  instrumented timing is not used as performance evidence.
- No affinity/governor changes; temperature/frequency were not sampled. This
  laptop has substantial between-run variation, especially in large queues.

The [complete timing estimates](formation-performance-2026-09-12.csv) retain all
41 screening, 10 confirmation and four final retained-source checks, including
regressions, 95% mean
confidence intervals and sample standard deviations. The table below uses the
confirmation **mean**, which can differ from Criterion's displayed slope.

| Workload | Before, µs (95% CI) | Candidate, µs (95% CI) | Mean change | Decision |
| --- | --- | --- | --- | --- |
| Anti-entropy last record, 10,000 remote peers | 3.021 (2.990–3.057) | 0.508 (0.503–0.515) | −83.2% | Keep |
| Anti-entropy 1,000-record reply, 1,000 remote peers | 444.128 (436.412–453.579) | 405.495 (401.874–409.293) | −8.7% | Keep |
| Worker Ping, 10 supplied gossip records, trimming required | 121.319 (119.892–122.914) | 41.663 (41.159–42.192) | −65.7% | Keep |
| Worker Ping, no gossip | 0.628 (0.620–0.637) | 0.606 (0.596–0.618) | −3.4% | Control; no substantial gain claimed |
| Gossip selection, 16 entries | 5.444 (5.372–5.524) | 4.650 (4.615–4.685) | −14.6% | Reject candidate overall |
| Gossip selection, 512 entries | 19.488 (18.107–21.204) | 16.255 (16.149–16.364) | −16.6% | Reject candidate overall |
| Gossip mixed priorities, 4,096 entries | 175.876 (173.625–178.503) | 196.096 (184.196–209.910) | +11.5% | Regression; reject |

The screening sweep also covered first-page collection and late cursors at
4/19/100/1,000/10,000 remote peers; three queue patterns at
16/128/512/4,096 entries; worker codec cases at 1/10/100 deltas; wire cases with
0/1/10 supplied gossip records; and summaries at 1/5/20/1,000 total members.
Codec and summary controls showed no material consistent improvement.
After reverting the gossip candidate, four final checks confirmed that the
retained source still improved the affected paths (about 0.43 µs for the late
cursor and 31.4 µs for the trimmed Ping). Between-run timing changed noticeably;
we retain the more conservative confirmation percentages above rather than
attributing the additional variation to the source change.

## Experiments and trade-offs

1. **Gossip selection — rejected.** Keep mutable references to selected entries
   instead of cloning their keys and looking them up again. This saved eleven
   allocations per non-retiring ten-record selection and improved small queues.
   However, large-queue results varied, the confirmation mixed-priority case
   regressed, and retaining candidate storage through output construction raised
   peak temporary heap use. The original implementation remains in place.
2. **Anti-entropy collection — retained.** Binary-search the sorted leaves in
   the cursor's bucket rather than scanning the already-consumed prefix. Borrow
   the last emitted key and clone it only for an incomplete reply. Cursor seeking
   changes from O(leaves before cursor) to O(log leaves in that bucket); traversing
   requested buckets and copying emitted records remain necessary. A complete
   reply no longer allocates a cursor key per record. Tree hashing, bucket order,
   completion rules and public return types are unchanged.
3. **Datagram trimming — retained.** Validate the full offered envelope, measure
   each removed delta once, and encode the final retained prefix once. The
   validated maximum of ten gossip records keeps every CBOR array header one
   byte, so each removal subtracts exactly that record's encoded length. The
   former repeated shrinking-envelope serialization could do quadratic work in
   the supplied record count; the new work is linear in supplied encoded bytes.
   The same trailing records are removed, in the same order; only individually
   fitting records receive deferred-gossip feedback. Unfit records still rely on
   reliable anti-entropy. Invalid offered records still fail before trimming.

DHAT totals for **1,000 calls**, with fixture construction before profiling:

| Workload | Blocks before → candidate | Allocated bytes before → candidate | Peak live bytes before → candidate |
| --- | --- | --- | --- |
| Gossip, 512-entry retained queue, take 10 (**rejected**) | 133,000 → 122,000 | 13,782,000 → 13,292,000 | 8,682 → 13,292 |
| Complete 1,000-member anti-entropy reply | 13,009,000 → 12,009,000 | 854,076,000 → 844,076,000 | 517,696 → 517,676 |
| Ten-record Ping with trimming | 473,000 → 271,000 | 102,003,000 → 41,714,000 | 14,625 → 18,001 |

The worker candidate reduces allocation churn but raises transient peak memory
by 3,376 bytes in this fixture while the original encoding and omitted-record
bookkeeping coexist. Both are bounded by the existing profile. The allocation
wire case includes cloning owned input, unlike Criterion's batched setup.

## Correctness and validation

The new collection reference test compares linear traversal against cursor
seeking at real, absent and boundary keys, zero and unbounded result counts, and
repeated/out-of-order/missing buckets. The datagram reference test compares exact
encoded bytes, omitted count, feedback order and errors with the original
repeated-encoding algorithm for 0–11 mixed-size records, including an invalid
record that would otherwise be removed. Accepted datagrams also pass the real
authenticated decoder. Existing digest goldens, protocol fixtures and owner
feedback tests remain in force.

Validation:

- `cargo fmt --all -- --check` and `git diff --check`: passed.
- `cargo clippy --locked -p orishu-membership -p orishu-worker --all-targets --all-features -- -D warnings`: passed on the retained source.
- Both crates' all-target/all-feature suite passed before the gossip revert.
  On the retained source, membership, worker library, standalone integration,
  benchmark smoke and example checks passed; six existing tests remain ignored.
- One retained-source worker binary run failed
  `diagnostics::tests::http_catchup_failure_stays_live_unready_and_recovers`:
  `catchup_started` was 2 instead of 1. The test passed in the preceding full
  run and on an isolated retry. Its timing-sensitive assertion remains an
  unresolved test-stability limitation; no diagnostics code was changed.
- `cargo test --locked -p orishu-membership -p orishu-worker --all-features --doc`: passed.
- `make docs-check`: passed. The initial sandboxed library run could not open
  local sockets; socket-bearing tests were rerun with the necessary access.
- Validation is scoped to these two crates and their dependency contracts;
  unrelated workspace/UI suites, sanitizer fuzzing and physical Pi runs were
  not rerun for this performance-only change.

## Reproduction

Use the [membership harness](../../crates/orishu-membership/benches/README.md) and
[worker harness](../../apps/orishu-worker/benches/README.md). Save a baseline from
the before source, then build and compare the same inputs on the after source:

```sh
cargo bench --locked -p orishu-membership --bench membership -- \
  'gossip_queue|anti_entropy_collect' --warm-up-time 0.5 \
  --measurement-time 1 --sample-size 20 --save-baseline rounds-start
cargo bench --locked -p orishu-worker --bench formation -- \
  'worker_wire|worker_codec|worker_summary' --warm-up-time 0.5 \
  --measurement-time 1 --sample-size 20 --save-baseline rounds-start
# On the candidate source, replace --save-baseline with --baseline.
# Confirmation uses --warm-up-time 1 --measurement-time 2 --sample-size 30.
cargo build --locked --release -p orishu-worker --example formation-allocations
# Run separately, moving dhat-heap.json after each invocation:
target/release/examples/formation-allocations collect 1000
target/release/examples/formation-allocations wire 1000
```

Local raw logs, frozen binaries and DHAT profiles are under
`target/performance-rounds-2026-09-12/`; they are disposable build artifacts.
The CSV and this report are the retained evidence. No Raspberry Pi experiments
were performed. These CPU/allocation results do not establish network capacity,
convergence latency or throughput on Pi hardware. Physical validation remains a
separate follow-up in the [formation performance task](../tasks/harden-formation-fuzz-performance.md).
