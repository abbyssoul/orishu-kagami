# Four-Pi / sixteen-worker API capacity — 2026-09-11

Status: **seven timed cells completed; highest observed aggregate 86,187.8
requests/sec; colocated CPU/generator limit, not isolated worker capacity**.
Owner: [physical formation experiment](../tasks/implement-physical-formation-experiment.md).
Operator recipe: [capacity diagnostic](../testing-worker-pi-cluster.md#four-hosts--sixteen-workers-capacity-diagnostic).
Previous evidence: [one worker per Pi, 500 requests/sec](formation-pi-pilot-2026-09-11.md).

## What was measured

Four worker processes on each of four physical Pis, **one sixteen-member
formation**, with distinct private state/client sockets and peer ports
9000–9003. Hosts are two Pi 5s with 8 GB, one Pi 4 Rev 1.4 with 8 GB, and one
Pi 4 Rev 1.1 with 4 GB. All run the supplied Ubuntu ARM64 installation. Each
worker had five process threads; each host's Rust probe had three. Four worker
processes are not four single-threaded executors or automatic core pinning.

The workload is the public typed **cluster-summary API over local Unix
sockets**, while authenticated QUIC membership crosses the actual Ethernet
network. It is not peer-message throughput, a remote-client TLS benchmark,
simulation throughput, or instrumentation-overhead acceptance. The unchanged
worker includes observability capabilities but disables runtime metrics,
tracing and logging. No Collector or scraping was used.

The requested 5,000 baseline was interpreted explicitly as **requests/sec per
worker**, hence 80,000 across the cluster. A bounded-concurrency generator
counts missed scheduled arrivals without queuing or replaying them. A missed
arrival is **not** a request rejected by Orishu. Unpaced cells have no offered-
rate delivery gate; their `delivery_met: false` field is not a failed rate test.

## Results

Every window lasts ten seconds. Fixed-rate cells use 32 clients per worker;
unpaced cells vary concurrency. These are sum-of-worker completion rates, not
pooled latency percentiles.

| Cell | Profile | Clients/worker | Aggregate offered/sec | Completed/sec | Outcome |
| --- | --- | ---: | ---: | ---: | --- |
| 0 | 5,000/sec/worker | 32 | 80,000 | 56,294.0 | Pi 4s cannot deliver the offer |
| 1 | 2,500/sec/worker | 32 | 40,000 | 37,521.2 | Pi 4s remain below the 99% delivery gate |
| 2 | 1,250/sec/worker | 32 | 20,000 | 19,988.5 | Every worker delivers at least 99.872% |
| 3 | Unpaced | 2 | — | 72,989.6 | Bounded-concurrency throughput |
| 4 | Unpaced | 8 | — | 85,414.7 | Best concurrency in the initial sweep |
| 5 | Unpaced | 32 | — | 81,420.1 | More concurrency reduces throughput and raises latency |
| 6 | Unpaced confirmation | 8 | — | **86,187.8** | Aggregate differs from cell 4 by 0.91% |

The predeclared conditional 64-client step was not taken: 32 clients did not
improve throughput over eight. The eight-client confirmation was planned,
not a discarded failed cell or an automatic admission retry. Two nearby
aggregate results are useful short-window evidence, not a sustained-load SLO
or a full repeatability/noise qualification; individual hosts still vary.

At the 80k/sec offer, the Pi 5s complete 19,991.6 and 19,987.5/sec, essentially
their entire shares. The Pi 4s complete 8,337.1 and 7,977.8/sec, with worst-worker
p95 around 19.66 and 20.70 ms. The verified equal-offer point is 1,250/sec/worker;
its deliverability boundary is only bracketed between 1,250 and 2,500 for this
profile, not located exactly. Faster hosts can do more when offered more work.

### Best observed aggregate window, cell 6

CPU below is a percentage of **each host's four-core capacity**, calculated
from each process's own measured CPU/time bracket. It is not instrumentation
overhead. The worker column sums that host's four workers. Latency is the
largest of their four request p95s, **not** a combined host p95.

| Physical role / hardware | Worker roles | Requests/sec | Worst-worker p95 (ms) | Workers CPU | Probe CPU | Supervisor CPU |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| 0 — Pi 5, 8 GB | 0–3 | 36,917.9 | 1.621 | 48.14% | 45.01% | 0.60% |
| 1 — Pi 5, 8 GB | 4–7 | 33,907.3 | 1.815 | 48.29% | 44.33% | 0.61% |
| 2 — Pi 4, 8 GB | 8–11 | 7,922.1 | 6.050 | 43.74% | 45.97% | 2.40% |
| 3 — Pi 4, 4 GB | 12–15 | 7,440.5 | 6.362 | 42.60% | 47.04% | 2.57% |

Known worker/probe/supervisor processes consume roughly 92–94% of each host's
four-core capacity. The two-executor-thread probe alone consumes about
1.77–1.88 cores per host. This is strong evidence of a **shared-host CPU and
generator ceiling**, not proof that the workers alone cannot serve more.
At 32 clients/worker, worst-worker p95 reaches 5.53–6.18 ms on the Pi 5s and
22.61–24.04 ms on the Pi 4s, despite lower aggregate throughput than eight.

The earlier desktop figure was approximately **128.9k aggregate requests/sec
with three workers and six unpaced clients**, not ten workers or 128k measured
peer messages. It also had a thermal/frequency slowdown; see the
[original diagnostic](formation-baseline-diagnostic-2026-09-10.md). Different
hardware, topology, generator count and conditioning prevent treating these
two figures as an isolated worker-scaling comparison.

## Failure modes and preparation fixes

Four supervised attempts are retained; none is overwritten:

| Attempt | Elapsed (s) | Result |
| --- | ---: | --- |
| `q-f448ec294a` | 15.796 | Coordinator keyword collision before any worker start or load. Exact dispatch-path regression added and fixed. |
| `q-0952b91e3e` | 37.293 | Sixteen workers form; final host's probe exits during warmup; no timed window. |
| `q-e65727d074` | 42.528 | Same warmup stop with bounded diagnostics; local membership remains sixteen/live; probe reports a transport error. |
| `q-d674280baf` | 207.372 | Construction-separated probe completes seven timed cells with clean cleanup. |

The instrumented warmup failure's 89-byte stderr digest matches the probe's
`TransportError` for the summary request. The shared client adapter flattens
the underlying request error, so this receipt alone does not prove its exact
transport cause. No failed warmup is relabelled a measured throughput limit.

The initial capacity probe built clients synchronously inside asynchronous
request tasks. With 128 clients on one host and two executor threads, later
client construction could delay already-started warmup requests. The revised
probe finishes **all client construction before spawning any warmup request**.
The one-second request timeout and timed workload remain unchanged. A focused
test verifies that the complete 128-client construction phase needs no running
server. The subsequent hardware run succeeds. This supports the startup-
starvation explanation; it is a probe fix, not a worker optimization or proof
that an allocator caused a worker regression.

During all seven successful timed cells there are **zero transport errors,
zero invalid membership/identity responses and no sample-cap hits**. The
observed stress response is latency growth, declining throughput at excess
concurrency, and missed offered arrivals in the bounded generator—not a crash,
server rejection, formation loss or catch-up failure. Pushing an independent
server-side overload source to failure remains a different experiment.

## Resource, timing and cleanup qualification

- All sixteen workers retain exact formation/identity/fingerprint/live
  predicates before and after every timed cell; every successful load response
  also validates its expected source, membership count, readiness and policy.
- All **2,800 process-sampling rounds** and **280 one-second environment
  snapshots** are present. The 168 process CPU brackets span
  10.001962373–10.023805263 seconds; sample/observer costs belong to their
  owning processes, not the workers.
- Sampled CPU temperature peaks at **52.095°C**. Firmware flags are zero
  before/after every window; no process swap is observed. The four workers
  together occupy roughly 81.5–85.5 MiB sampled RSS per host. There is no
  evidence of a 4 GB RAM-capacity limit. Stable RSS does not exclude hot-path
  allocation churn, which was not profiled on these Pis.
- Conditional start-skew bounds are **61.053–63.919 ms**, with nominal common
  windows at least **9.936 s**, under the recorded offset/drift assumptions
  and explicit 20 ms change allowance. These are not continuously qualified
  wall clocks or a proof of each client's continuous activity.
- Host RX-drop counters again grow by **4–5** in each approximately nine-second
  sample bracket. Three hosts' TX counters still advance little or not at all.
  Counter attribution/freshness remains unresolved; do not call these Orishu
  packet losses or infer Ethernet saturation from them.
- Final run cleanup is clean: all sixteen workers exit 0; every measured probe
  exits 0 and its receipt is retained. An independent read-only audit of
  **all four attempts on all hosts** finds no run-owned processes or sockets.
  Earlier coordinator `cleanup_clean: false` outcomes remain unchanged; the
  later audit separately resolves resource reclamation. Failed probes and
  poisoned RPC receipts are not successful measurements.

Supervised execution totals **302.989 seconds**; the final run is within its
remaining 400-second cap and all attempts fit the stated ten-minute batch
ceiling. Native builds (9.51 and 9.79 seconds reported by Cargo) are excluded.
Read-only prechecks/audits and staging are separate ancillary activities, not
timed load. No system tuning, package installation, production source change,
existing-worker termination or repository commit was performed. Original
staged binaries, private inventories and failed-run evidence are preserved.

## Reproduction and evidence

Native source: HEAD `9c11d9854dc22dd3f999571bed4424ac144a505a` plus the probe
example and its capacity module. Rust 1.97.1, LLVM 22.1.6,
`aarch64-unknown-linux-gnu`, release, `observability,otlp-tracing`, locked/offline
dependencies, two build jobs. The final source directory on the build Pi is
`/home/soultaker/orishu-lab/capacity-build.By9BA2/source-v2`.

| Artifact | SHA-256 |
| --- | --- |
| Unchanged worker | `1553cc178bb09edbc53a6663a346ca846459de1f5ba14f95f6c40dd82230369d` |
| Unchanged CLI | `edf3d53be54586495cbd5311596bafd2c04ded653515935c6965d90811fd2aa5` |
| Initial capacity probe | `b41be34f5f9fb458a8ac7ecef1f0d00bfc7a09304cc73dc19bea3eb353d1895a` |
| Construction-separated probe | `651e2892d068b3b0a9e17698c07d6d440cf6973a7f3086829639bb6cc23b827d` |
| Final Rust source archive | `4fd3336f049e634866651f0b3f8dff5fae5d9ec3562d29b54a590b6652cafdf3` |
| Final harness archive | `5209f49f2cd11aa72ee27a50abcd463c7ffbe019f4ea2bef43888242f190c3b8` |
| Final node-session bundle | `e6cdb2fa3e506b6c471cfcf76dc905d233bf7f62883b48511ef4ed43917c072e` |
| Final coordinator source | `e236d381033be9ac40cd981e27867cca46e530843f8bb30e1c63b42c6587890c` |
| Final result | `7e548ed8cc54c32bc169755e69f68b0a2a0ea9ff85dccf7cf3f1debd1e23ebef` |
| Retained warmup failure receipt | `bb7f2a8fe4139df1b6484f7cd6c79639c4032caf0563d21debcea5f788825a55` |

Private evidence lives under `/tmp/orishu-pi-capacity.HLmYgQ/`: all four run
directories, source/harness archives, build manifests, `cleanup-audit.json`
and `evidence-index.json`. The separately fetched `warmup-failure-pi4-4gb.json`
retains the failed probe's classification, public membership snapshots and
cleanup. The index hashes all 28 host-window reports and
key aggregate artifacts. An independent offline pass revalidates raw load
identity/accounting, aggregate rates, resource arithmetic and receipt equality.
These `/tmp` artifacts are machine-local, not durable published release evidence.

Checks: **141 Pi-harness tests, 36 legacy formation-harness tests, 14 Rust
probe tests, targeted Clippy, formatting, documentation and diff checks**.
Socket-dependent tests need normal host permissions; sandbox-denied runs were
repeated successfully with those permissions. Full workspace tests were not
run for this lab-tooling change.

## Next useful experiment

To measure the worker-side ceiling, move load generation off the Pis using a
separately defined authenticated network-client profile. This frees nearly
half each host's CPU, but introduces client transport/TLS costs, so preserve
this colocated result as its own baseline. Alternatively refine the equal-rate
1,250–2,500 bracket if that is the operational question. Neither follows
automatically from this batch. No operator action is required to use these
results; a new experiment should select its workload, security and allowance.

M4's paired observability acceptance, the NIC-counter qualification and
the thirty-worker performance/noise gates remain open. This capacity diagnostic
does not waive them or claim that four workers per Pi is optimal.
