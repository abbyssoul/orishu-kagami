# Sixteen-worker authenticated network capacity — 2026-09-11

Status: **nine windows completed; target not met; end-to-end capacity finding,
not an isolated Pi ceiling or M4 observability acceptance**.

## Result and scope

Four workers on each of two Pi 5s and two Pi 4s formed one sixteen-node
formation. Four native desktop generators used the public typed HTTPS client,
with verified certificates and worker-local operator bearer credentials.
The Pis ran no load generator. Worker binaries, runtime thread defaults,
affinity, governors, services and firmware settings were unchanged.

Highest observed aggregate: **36,751.8 successful summary requests/sec**, with
64 persistent clients per worker; the confirmation was **36,552.3/sec**
(0.54% lower). The 5,000 requests/sec **per worker** target offers 80,000/sec;
that baseline delivered 33,160.7/sec. This counts cluster-summary requests,
not simulation operations or QUIC peer messages.

Important qualification: all Pis use Ethernet, but the desktop route was
**Wi-Fi**, `wlp0s20f3`; its Ethernet adapter had no carrier. No route or host
configuration was changed. The result measures this complete client path,
including TLS, authentication, Wi-Fi and worker response processing. It does
not establish either Wi-Fi saturation or maximum all-wired Pi throughput.

Later qualification: the [wired-desktop comparison](formation-pi-wired-comparison-2026-09-11.md)
found three Pis selecting `wlan0` for replies even from their Ethernet IPs.
Thus Ethernet link presence alone was insufficient to characterize the Pi
traffic path. Those later route observations are not a retrospective packet
capture, but qualify the earlier assumption that all Pi traffic used Ethernet.

## Workload and safety

- Existing `orishu-worker`/`orishuctl` artifacts, telemetry compiled in but
  metrics/tracing/logging disabled, no simulation workload.
- Four worker processes per host; each observed five OS threads (four Tokio
  executor threads plus main). The Pi supervisor had five threads. Each of the
  four desktop probes uses a two-thread executor. Async IO is not necessarily
  single-threaded; the [post-PoC review](../tasks/review-worker-executor-performance.md)
  separately tracks executor sizing and scheduler/CPU/allocator hypotheses.
- Ten-second windows, public typed response validation for exact formation,
  source node, sixteen live members, unlocked policy and introducer readiness.
  Full membership/pin checks before and after every cell.
- Fixed-rate windows use 32 clients/worker, absolute integer arrival slots,
  one request in flight per client and a one-second request timeout. Late
  slots are skipped, never queued/replayed. A delivery pass requires at least
  99% of offered arrivals on **every** worker. This is a lab delivery criterion,
  not a latency SLA. Existing 500/sec observability acceptance is unchanged.
- Unpaced windows use 2/8/32 clients; the >5% improvement at 32 permits the
  existing 64-connection ceiling, followed by one explicit best-cell repeat.
  No failed cell is discarded or automatically retried.
- One 600-second batch ceiling excluding builds, including cleanup reserve;
  actual elapsed **249.966 seconds**. No failed preparation attempts occurred.
  Worker watchdogs remain bounded at 600 seconds, individual probes at 60.

Temporary TLS listeners bound only each inventory IP at ports 9440–9443,
alongside local admin sockets and QUIC ports 9000–9003. Each fresh server
certificate carried its IP SAN and was trusted through verified SSH. Every
endpoint rejected absent/wrong tokens, an untrusted certificate and a wrong
server identity before load. Positive typed-client warmup then succeeded.
There was no insecure TLS, plaintext or forwarded-Unix fallback. Server keys
stayed on their Pi; copied operator credentials used private coordinator files,
not argv, environment or published results, and were removed at cleanup.

## Rate and concurrency curve

All rates below are aggregate over sixteen workers. The p95 column is the
**worst individual worker's p95**, not a pooled or averaged percentile.

| Cell | Offer/worker/sec | Clients/worker | Completed/sec | Minimum worker delivery | Worst worker p95, ms |
| --- | ---: | ---: | ---: | ---: | ---: |
| 0 | 5,000 | 32 | 33,160.7 | 32.006% | 33.093 |
| 1 | 2,500 | 32 | 28,319.1 | 57.368% | 41.404 |
| 2 | 1,250 | 32 | 19,056.6 | 91.152% | 21.458 |
| 3 | 625 | 32 | 9,983.7 | **99.792%** | 16.086 |
| 4 | Unpaced | 2 | 7,158.5 | N/A | 8.952 |
| 5 | Unpaced | 8 | 18,375.5 | N/A | 14.343 |
| 6 | Unpaced | 32 | 33,275.1 | N/A | 30.950 |
| 7 | Unpaced | 64 | **36,751.8** | N/A | 52.193 |
| 8 | Unpaced confirmation | 64 | **36,552.3** | N/A | 54.794 |

The raw unpaced `delivery_met: false` means **not applicable** because there
is no offered rate. It is not a failed fixed-rate test. One confirmation does
not qualify general repeatability. The curve brackets observed deliverability;
it does not identify an exact sustainable threshold between 625 and 1,250.

At the best aggregate cell (7), each host contains four workers:

| Host | Model / memory | Completed/sec | Worst worker p95, ms | Four workers' share of four-core host CPU |
| --- | --- | ---: | ---: | ---: |
| `pi1` | Pi 5 / 8 GB | 10,943.4 | 36.613 | 35.88% |
| `pi2` | Pi 5 / 8 GB | 10,079.4 | 38.786 | 42.01% |
| `pi3` | Pi 4 / 8 GB | 8,208.0 | 48.076 | 85.11% |
| `pi4` | Pi 4 / 4 GB | 7,521.0 | 52.193 | 82.89% |

Host labels are anonymized consistently with the
[wired-comparison report](formation-pi-wired-comparison-2026-09-11.md#outcome);
they are not SSH hostnames. Measurements are unchanged.

CPU percentages use each process's high-resolution CPU time divided by its
own measured wall bracket and four host cores. They exclude the separately
recorded Pi supervisor. The four desktop generators together consumed about
4.21 cores at cell 7 and 4.64 at confirmation, out of twenty logical CPUs;
this is not evidence that each generator's hot execution path has spare capacity.

## Failure mode and limits of attribution

There were **zero timed transport errors, invalid responses or sample-cap
hits**, and no before/after membership failure. All measured probes and all
workers exited zero. The observed degradation was increasing response latency
and skipped fixed-rate arrivals; the bounded generator cannot keep delivering
the requested schedule as responses take longer. With 64 clients, throughput
improved only about 10.4% over 32 while worst-worker p95 rose from 31 to 52 ms.
We reached the configured connection ceiling, not a crash threshold.

The Pi 4s show substantial worker-side CPU pressure. The Pi 5s do not show
aggregate host-CPU saturation, despite substantially lower throughput than
their earlier colocated Unix runs. These observations support further
separation of worker CPU, single-path contention, network and client costs;
they do not identify one proven root cause. Moving the generator off-host
simultaneously changed transport, TLS/authentication and generator placement,
so this is not a controlled measurement of TLS overhead alone.

The [colocated result](formation-pi-capacity-2026-09-11.md) reached 86.2k/sec,
but its generators consumed nearly half each Pi's CPU. Unix is a favourable
transport, not a rigorous upper bound when that colocated cost is included.
Neither run proves the desktop thirty-worker noise hypothesis, which remains
explicitly unverified in the post-PoC task.

## Measurement quality and cleanup

- 36 host windows, 3,600 process-sampling rounds, no missed process or kernel
  environment samples. Worker/supervisor sampling was 100 ms; kernel
  environment sampling was one second. Desktop generator accounting and CPU/
  temperature observations are separately retained.
- Maximum sampled Pi temperature **51.608°C**; all pre/post firmware flags
  zero. Maximum sampled desktop core temperature **65°C**. Desktop throttling
  counters were not collected, so no continuous desktop thermal claim is made.
- No sampled process swap. Maximum summed four-worker peak RSS 91.42 MiB;
  largest generator peak RSS 53.33 MiB. These bounds do not measure allocation
  churn or prove allocation-free request paths.
- Actual desktop probe start skew 0.67–3.77 ms. Offset-mapped Pi sampler and
  generator starts passed the conditional <100 ms joint bound using the
  existing explicit 20 ms offset-change allowance. This is a nominal-window
  overlap check, not continuous clock or per-client activity qualification.
- RX-drop increments of 4–5 per roughly nine-second kernel bracket persisted.
  Three hosts still reported implausibly tiny TX-byte increments despite
  successful HTTPS responses. These unresolved host-counter findings cannot
  prove Orishu packet loss or link saturation; retain them rather than invent
  a bandwidth estimate from those counters.
- Independent SSH audit found no owned worker, client socket or capacity TLS
  listener left behind on any Pi. All original staged worker/CLI hashes were
  unchanged. No local generator remained, and all sixteen copied tokens and
  certificates were removed. Private remote run evidence and server state
  remain; no user data or prior experiment evidence was deleted.

## Reproduction and evidence

Run ID `q-59ba79d93c`. Private local evidence root:
`/tmp/orishu-pi-network.nVmWMS`; each Pi's private worker directories:
`/home/soultaker/orishu-lab/runs/q-59ba79d93c-0` through `-3`.
These are session-local evidence locations, not committed artifacts.
The private inventories remain uncommitted.

Command: use the [network capacity recipe](../testing-worker-pi-cluster.md#off-host-authenticated-https-capacity-profile)
with `--baseline-per-worker 5000 --max-elapsed-seconds 600`, the populated
four-host inventory and the existing pilot start policy. The native release
probe was built with Rust 1.97.1, LLVM 22.1.6, x86_64-unknown-linux-gnu and
features `observability,otlp-tracing`. Worker binaries were not rebuilt.

| Evidence | SHA-256 |
| --- | --- |
| Native network probe | `97c07b640328839fb22d7d8f7be73680bc75af479e3e18aed8d0d77e64abb694` |
| Source archive (HEAD plus probe overlays) | `69cacd64aecd67d22522d641bff4cc89a263afe9199cc15aee6b6b58d446153e` |
| Frozen harness archive | `e963fe925b274ca7d77b6fcbe4d5d33adef30cf52a16692cae352174d6bb5ee4` |
| `run/result.json` | `3cc19400dbedf08c7b701de234fe51506e66931d22a1843b463c45a7029822f7` |
| Independent cleanup audit | `45f1751589e5d40a4d056e2843962486983ebbe52a36301c61646279abc99ce4` |
| Index hashing all 36 host windows and principal evidence | `03af2fc470f178ae9b80e3f5919e99fb40f18f4c99e8b1fbd8a8a393d207ed24` |

An offline recheck matched each window to its retained file, revalidated
profiles/identities/accounting, recomputed aggregate QPS and delivery decisions,
and checked security receipts and successful generator exits.

Validation: 148 Pi-harness tests and 36 existing telemetry tests passed; 15
Rust probe tests, targeted probe Clippy with warnings denied, formatting and
documentation checks passed. Socket tests were run with normal host permissions
after a sandbox-only `EPERM` in a collector socket test. Full-workspace Cargo
tests were not rerun for this lab-tooling-only increment. No production worker,
client protocol, dependency or persisted formation format was changed.

## Next useful experiment

Problem: determine whether 5,000 requests/sec/worker is achievable and which
component limits it. Tried: colocated Unix and now off-host authenticated
HTTPS over the available Wi-Fi route. Next preferred comparison: connect the
desktop to wired Ethernet and repeat this unchanged network profile, retaining
mixed-model CPU and latency results. This removes one major confounder but
does not promise the target will pass. Alternative: a separate wired load
generator; placing it on a tested Pi would reintroduce CPU competition.

User action for that comparison: provide a working wired coordinator path (or
a separate wired generator). No such action is required to retain these
findings or continue PoC planning. Runtime/thread optimizations remain in the
separate post-PoC review rather than silently changing this baseline.
