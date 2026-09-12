# Harden formation parsing and measure local performance after M4

Status: **local fuzz/benchmark infrastructure implemented; three local
performance experiments measured, two optimizations retained; sanitizer
campaign run with no parser or transition defects found**. M4's accepted scope
is unchanged.

## Outcome and current gap

Exercise hostile membership messages through both the core's serde contracts
and the worker's actual bounded CBOR/profile decoders. Make panics, excessive
work and allocation failures reproducible. Establish CPU and allocation
benchmarks before retaining performance changes, following the
[performance guide](../high-performance-rust.md).

The initial core benchmarks covered merge, probe, digest, anti-entropy,
admission and queue selection, but not parsing, rejection or sustained reuse.
Queue teardown was incorrectly included in selection timing. The worker lacked
Criterion and allocation workloads for its formation adapters.

## Delivered slices

- An isolated [fuzz workspace](../../fuzz/README.md), deterministic public seeds,
  four parser/transition targets, bounded smoke command and CI job.
- Expanded [membership benchmarks](../../crates/orishu-membership/benches/README.md),
  including corrected queue timing and the five-/twenty-member formation scale.
- [Worker benchmarks and DHAT example](../../apps/orishu-worker/benches/README.md)
  for framing, rejection, authenticated peer traffic and summary serialization.
- A [four-round sanitizer campaign](../measurements/formation-fuzz-campaign-2026-09-12.md)
  over all four targets: 66.8 million executions, no crash, timeout or
  allocation failure. It found one harness defect rather than a product one —
  the smoke command never approached the frame ceiling — now fixed by a second
  `-len_control=0` pass, with the depth guard promoted into an ordinary test.
- [Three local performance experiments](../measurements/formation-performance-2026-09-12.md):
  retain faster anti-entropy collection and datagram trimming; reject the gossip
  selection candidate after a large-queue regression. Includes timing intervals,
  allocation counts, reference tests and reproduction commands.

## Acceptance

- Run every seeded target with sanitizer instrumentation and explicit work/RSS
  limits; retain failures and promote minimized inputs into ordinary tests.
- Measure optimized before/after inputs with no concurrent builds or fuzzing;
  report absolute intervals, variance and allocation evidence.
- Preserve framing caps, validation, ordering, retirement and authority checks;
  run production-interface regression and formation integration tests.
- Keep dependency isolation, lockfiles, formatting, lints and documentation valid.

## Hardware handoff and non-goals

The operator explicitly limited this round to local work; **no Raspberry Pi
cluster runs** belong to this task's local result. The
[Pi client-error diagnostic](../measurements/formation-pi-client-errors-2026-09-11.md)
motivates separate summary and peer workloads and separating generator pressure.
Its throughput changes are not attributable to these code changes.

After reviewing local results, compare the retained changes on real Pi 4/5
hardware through the existing [physical experiment](implement-physical-formation-experiment.md).
Freeze binaries, feature sets and load; retain identity/convergence, client
errors, timing/delivery gates, worker/generator pressure, scheduler and thermal
receipts, failed cells and cleanup. Repeat fixed-rate and saturation cases.

No executor policy, host networking, affinity/governor, scientific semantics or
persisted/wire format change is part of this local task. The
[executor review](review-worker-executor-performance.md) remains independent.
