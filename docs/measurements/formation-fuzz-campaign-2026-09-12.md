# Formation fuzz campaign — 2026-09-12

Four rounds over all four targets produced **66.8 million executions and no
product defect**: no crash, sanitizer error, timeout or allocation failure in
`orishu-membership` or the `orishu-worker` peer codec, wire and catchup
adapters. The campaign did find one **harness** defect, described below, which
had been silently limiting what every previous smoke run could reach.

No wire, persisted format, dependency, authority or executor policy change
follows from this round.

## Toolchain and method

- Source revision **`c288c0a`** plus the incoming dirty worktree, unchanged
  during the campaign.
- `rustc 1.99.0-nightly (84b36a78a 2026-08-06)`, `cargo-fuzz 0.13.2`,
  `libfuzzer-sys 0.4.13` pinned by the fuzz lockfile.
- **AddressSanitizer** (cargo-fuzz default) with `overflow-checks = true` from
  the fuzz release profile, so arithmetic overflow panics are campaign failures.
- Limits per run: `-timeout=10 -rss_limit_mb=2048 -max_len=1048581`.
- Intel Core i7-1370P, x86-64 Linux 6.17.0-41-generic, 20 logical CPUs. Targets
  ran concurrently, four at a time; no benchmark ran during the campaign.
- Corpus reseeded from `examples/seed_corpus` beforehand and carried forward
  across rounds, so each round starts from the previous round's coverage.

## Rounds

| Round | Per target | Configuration | Executions |
| --- | --- | --- | --- |
| 1 | 180 s | default length ramp | 24,151,535 |
| 2 | 240 s | `-len_control=0` | 14,802,304 |
| 3 | 300 s | `-use_value_profile=1` | 10,214,393 |
| 4 | 600 s | `-len_control=0 -use_value_profile=1` | 17,590,638 |
| | | **total** | **66,758,870** |

Final edge coverage: `membership_messages` 5,507, `worker_peer` 3,745,
`worker_frames` 3,440, `worker_catchup` 2,262. Round four doubled round three's
budget and returned between 1.4% and 3.4% more edges per target, so further
rounds at this configuration have low expected yield; new findings should come
from new targets or assertions rather than longer campaigns.

## Finding: the smoke campaign never reached the frame ceiling

`-max_len=1048581` only *permits* inputs just above the one-MiB frame ceiling.
libFuzzer's default `-len_control` raises the length limit gradually, so a
smoke-length campaign never generates anything near the cap. Measured maximum
length limit reached in the 180-second round one, against the 1,048,581-byte
ceiling:

| Target | Round 1 (default ramp) | Round 2 (`-len_control=0`) |
| --- | --- | --- |
| `worker_peer` | 12,548 (1.2%) | 1,048,581 |
| `worker_catchup` | 18,183 (1.7%) | 1,048,581 |
| `membership_messages` | 68,830 (6.6%) | 1,048,581 |
| `worker_frames` | 81,513 (7.8%) | 1,048,581 |

The 30-second `make fuzz-smoke` default reached less than this. The README's
claim that oversized messages "are tested instead of filtered out by the
harness" was therefore true of the *limit* and false of the *campaign*: the
declared-oversize, `TooLarge` and datagram-cap paths were effectively unreached
in CI. `make fuzz-smoke` now runs a second `-len_control=0` pass per target,
and the README states the distinction. Both passes are kept, because the ramp
still finds small structural cases that full-length generation rarely produces.

## Retained artifacts and the promoted regression

The seven artifacts under `fuzz/artifacts/membership_messages/` predate this
campaign and **no longer reproduce** on this toolchain; they were replayed
individually and all passed. They record the `Hash256` hex decoder panicking on
a 64-*byte* bucket string containing a multi-byte character, which made
`&value[index * 2..index * 2 + 2]` slice across a char boundary. Deleting the
incoming `is_ascii_hexdigit` guard and replaying the artifact reproduces
`end byte index 14 is not a char boundary`, confirming that guard as the fix.
It arrived with the incoming worktree and is already pinned by
`wire::tests::malformed_text_digest_is_rejected_through_authenticated_wire_decode`,
which sweeps several multi-byte characters across every offset. No further
regression test is owed for it.

A neighboring guard in the same module *was* unpinned. Because `MerkleDigest`
carries a peer-supplied `depth` and `1usize << depth` overflows at or above the
usize width, the order of the two checks in `is_well_formed` is load-bearing
rather than stylistic: the range check must precede the shift. Reversing
them reproduces `attempt to shift left with overflow`, which was confirmed by
mutating the source and observing the new test fail. The regression now lives
in `antientropy::tests::every_peer_declared_depth_is_refused_without_shifting_by_it`,
covering every `u8` depth outside the valid range against several bucket counts
so that a matching length cannot mask the check.

## Caveats

A passing finite campaign does not prove all messages safe. These targets are
CPU-only: live QUIC/HTTP survival, TLS, slow streams, queue pressure and
shutdown remain integration-test concerns. `membership_messages` exercises the
core's public serde contract through JSON, not a network codec, so its coverage
number is not comparable to the worker targets'. Successful deserialization is
never domain acceptance.
