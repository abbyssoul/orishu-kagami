# Worker formation performance

Run `cargo bench --locked -p orishu-worker --bench formation`. Criterion measures
optimized production interfaces, using deterministic inputs:

| Group | Workload |
| --- | --- |
| `worker_codec` | Encode/decode stream frames with 1/10/100 member deltas; encoded-byte throughput |
| `worker_reject` | Duplicate fields, enormous declared arrays, depth overflow and a real oversized frame |
| `worker_wire` | Authenticated profile-5 Ping encoding/decoding with 0/1/10 supplied gossip records, including datagram trimming |
| `worker_summary` | Published owner view and actual client `ApiResponse` CBOR serialization at 1/5/20/1000 total members |
| `worker_catchup` | Capture a frozen golden-model baseline and receive its complete page sequence |

The [2026-09-12 measurements](../../../docs/measurements/formation-performance-2026-09-12.md)
record the datagram-trimming optimization, exact-byte reference checks and
before/after timing and allocation evidence.

Input cloning is outside timed regions for consuming APIs. Repeated result
allocation/deallocation is part of borrowing codec APIs. The summary owner and
runtime are established once per case; no HTTP/TLS or runtime startup is timed.
The 10-gossip decode case measures the *retained* packet after trimming, not ten
delivered records. Byte work and structural validation scale with encoded bytes;
summary encoding remains bounded independently of membership count.

The Pi campaign showed CPU saturation and coordinator sensitivity for summary
requests; these benchmarks isolate its projection/serialization cost without
claiming network capacity. Formation tests motivate the peer/datagram cases.
Use [membership benchmarks](../../../crates/orishu-membership/benches/README.md)
for transitions, anti-entropy and gossip selection.

Build first, stop other experiments, and retain a baseline:

```sh
cargo bench --locked -p orishu-worker --bench formation --no-run
cargo bench --locked -p orishu-worker --bench formation -- --save-baseline before
# Apply exactly one candidate, rebuild, then compare.
cargo bench --locked -p orishu-worker --bench formation -- --baseline before
```

Keep compiler, profile, features, inputs and runtime configuration identical.
Record CPU topology, available CPUs, temperatures/frequency where available,
absolute intervals and outliers, not just percentages. Criterion reports live in
`target/criterion`. Timing under concurrent compilation/fuzzing is exploratory
only. Repeat noisy comparisons; do not interpret microseconds as network p95.

For allocation evidence, separately run the DHAT example (the allocator is
linked into this example only):

```sh
cargo build --locked --release -p orishu-worker --example formation-allocations
mkdir -p target/formation-allocations
cd target/formation-allocations
../release/examples/formation-allocations decode 1000
../release/examples/formation-allocations frame 1000
../release/examples/formation-allocations gossip 1000
../release/examples/formation-allocations collect 1000
../release/examples/formation-allocations wire 1000
```

Move `dhat-heap.json` to a distinct name after each run to retain profiles.
Construction happens before profiling. The gossip queue survives all iterations
and uses a non-retiring hop budget. DHAT timing is not Criterion timing; total
allocation counts/bytes and stacks identify costs that stable RSS cannot show.
`collect` copies a complete 1,000-member reply from one retained tree. `wire`
encodes a ten-record Ping that needs datagram trimming; its input clone and
sender/formation ownership are included, unlike Criterion's batched input setup.

Hardware follow-up must use the existing Pi formation/capacity harness and its
identity, timing, delivery, error and cleanup gates. Compare fixed offered loads
and saturation, worker and generator CPU separately, and preserve failed cells.
Executor policy, affinity/governor changes, TLS throughput and end-to-end Pi
improvements require separate evidence. This local suite changes none of them.
