# Wired-desktop comparison and remaining Pi Wi-Fi routes — 2026-09-11

Status: **partial: four of six windows measured; request-error gate stopped
the repeat. Desktop wired, but three Pis still select wireless return routes.**

Follow-up: the approved [temporary route correction](formation-pi-ethernet-routing-2026-09-11.md)
completed all six windows with unchanged workers and a 94.28k/sec peak. It
preserves this run's failures; it does not retrospectively identify their cause.

## Outcome

Lab identities are anonymized consistently across the capacity reports:
`pi1`/`pi2` are the two 8 GB Pi 5s, `pi3` is the 8 GB Pi 4, and `pi4` is
the 4 GB Pi 4. IPs below are documentation-only replacements from
`192.0.2.0/24`, not experiment endpoints. Measurements and routing relationships
are unchanged; raw inventories and route receipts remain private.

Moving the desktop from Wi-Fi to Gigabit Ethernet improved observed aggregate
HTTPS throughput from 36,751.8 to **63,361.3 requests/sec** at 64 clients per
worker (+72.4%). This is one error-free unpaced wired-desktop window, not a
confirmed sustainable ceiling. The planned second unpaced pass was not run.
The 5,000 requests/sec/worker target was still not met cluster-wide.

The more important finding is that a plugged-in Ethernet cable did not ensure
an all-wired path. Post-run route lookups to desktop `192.0.2.100`, explicitly
using each worker listener's source IP, returned:

| Pi | Bound server IP | Selected return device | Physical Ethernet |
| --- | --- | --- | --- |
| `pi1` | `192.0.2.11` | `eth0` | 1,000 Mbit/s, full duplex, carrier up |
| `pi2` | `192.0.2.12` | **`wlan0`** | 1,000 Mbit/s, full duplex, carrier up |
| `pi3` | `192.0.2.13` | **`wlan0`** | 1,000 Mbit/s, full duplex, carrier up |
| `pi4` | `192.0.2.14` | **`wlan0`** | 1,000 Mbit/s, full duplex, carrier up |

All four inventory IPs are assigned to `eth0`; the problem is route selection,
not simply an inventory containing wireless IPs. Default-source lookups also
selected `wlan0` on those three Pis. Both interfaces have addresses on the
same subnet. These are retained **post-run routing observations**, not a
packet capture or continuous proof of every packet's path. They prevent an
all-wired interpretation and are consistent with the tiny `eth0` traffic
counters and the unequal performance gains. No route, service, affinity,
governor, executor setting or Wi-Fi state was changed by the experiment.

## Controlled conditions and actual windows

The approved plan was `(5,000/worker/sec, 32 clients)`, `(unpaced, 32)`,
`(unpaced, 64)`, repeated once, with ten seconds per window. The new
`--short-comparison` selector changes only window selection. Worker, CLI,
native release probe and remote helper bundle hashes matched the
[Wi-Fi run](formation-pi-network-capacity-2026-09-11.md) exactly. Four workers
per Pi, sixteen-node membership, authenticated HTTPS, request timeout, warmup,
runtime defaults and all safety gates were unchanged. No build was needed.

Desktop routes to all four Pis selected its wired interface (hardware-derived
name omitted) both before and after
the run; its link was 1,000 Mbit/s full duplex. Desktop Wi-Fi was down. The
independent Pi return-route check above was added after the unequal results.

| Profile | Previous Wi-Fi desktop, requests/sec | Wired desktop, requests/sec | Change | Wired worst-worker p95 |
| --- | ---: | ---: | ---: | ---: |
| 5,000 offered/worker/sec, 32 clients | 33,160.7 | 43,842.6 | +32.2% | 29.718 ms |
| Unpaced, 32 clients | 33,275.1 | 53,635.4 | +61.2% | 29.378 ms |
| Unpaced, 64 clients | 36,751.8 | 63,361.3 | +72.4% | 56.438 ms |
| Repeat 5,000 offered/worker/sec, 32 clients | 33,160.7 | 43,140.0 | +30.1% | 30.632 ms |
| Repeat unpaced, 32 clients | — | **Not run: safety stop** | — | — |
| Repeat unpaced, 64 clients | — | **Not run: safety stop** | — | — |

The final measured row includes the errors described below and is **not** a
clean confirmation. The first baseline's minimum per-worker delivery was
32.706%, so aggregate improvement does not establish the target on every node.
Percentiles are the worst individual worker's percentile, not an average.
Historical and current runs were sequential, not randomized paired trials;
fresh process identities and thermal/temporal conditions also differ.

At the error-free 64-client window, each row represents four worker processes:

| Pi | Requests/sec | Worst-worker p95 | Worker CPU share of four-core host |
| --- | ---: | ---: | ---: |
| `pi1` | 38,080.4 | 14.168 ms | 98.87% |
| `pi2` | 11,429.2 | 32.077 ms | 46.11% |
| `pi3` | 6,705.6 | 56.438 ms | 69.00% |
| `pi4` | 7,146.1 | 51.676 ms | 75.36% |

`pi1` moved from 10,943.4/sec in the comparable Wi-Fi window to 38,080.4/sec
and near-full worker CPU use. The remaining Pis did not make comparable gains.
This supports a substantial network-path influence and makes an all-wired
comparison useful. It does not identify the cause of the request errors or
prove a specific worker/runtime bottleneck. Keep executor optimization in the
[post-PoC review](../tasks/review-worker-executor-performance.md).

## Stop condition and diagnostic limit

The fourth window recorded **four `transport_errors`** against global worker
role 4 on `pi2`. Its retained row has 22,294 successful timed requests,
27,674 skipped arrivals, 28 tail requests and four errors, exactly accounting
for 50,000 offered arrivals. Other worker rows recorded no request errors.
There were no invalid responses or sample-cap hits, and exact membership
checks passed before and after every measured window, including this one.

The coordinator correctly stopped further windows under the existing
request-error gate. It did not retry, suppress the failed window, increase the
timeout or relax the gate. Total experiment elapsed was **126.292 seconds**,
within the 360-second ceiling; forty seconds of requested windows were measured.

The diagnostic workflow first retained and replay-validated the failure's
accounting. The probe maps public-client request errors to one counter and
does not retain their causes/timestamps, so **timeout, disconnect, API refusal
or another client error cannot be distinguished from this receipt**. A cause
must not be inferred from the field name. No speculative production fix or
additional load reproduction was attempted. The exact cause remains open.

Read-only route checks addressed the performance asymmetry separately:
remaining wireless routing was the first hypothesis; a degraded Ethernet
negotiation was the second; worker-side pressure was a third. Exact-source
lookups confirmed the first routing condition, while all physical Ethernet
links reported Gigabit/full duplex. This does not prove that Wi-Fi caused the
four request errors. A future failure investigation needs a bounded,
credential-safe error classification before drawing that conclusion.

## Measurement quality and cleanup

- All sixteen retained host-window files passed an offline profile, identity,
  count and aggregate-QPS recheck; the failed window was included.
- All security qualification checks passed: missing/wrong token, untrusted
  certificate and wrong server identity were rejected; positive warmup passed.
- 1,600 process-sampling rounds, no missed process samples. No sampled swap;
  pre/post Pi firmware flags were zero. Maximum sampled Pi temperature
  51.121°C, desktop core temperature 76°C. Desktop throttling was not measured.
- Maximum summed four-worker peak RSS 89.55 MiB; maximum generator peak RSS
  55.75 MiB. These are not allocation-rate measurements.
- Each Pi retained `(5,5,5,5,5)` worker/supervisor OS thread counts. Generator
  process accounting remained separate from workers. No executor policy change.
- Conditional generator/sampler start-skew bounds were 58.4–59.7 ms with the
  unchanged explicit clock allowance; no continuous packet/activity claim.
- The `eth0` counters still showed 4–5 RX drops per roughly nine-second
  bracket. Three Pis' near-zero Ethernet TX and very small RX are consistent
  with their wireless route selection: monitoring only the inventory-named
  interface cannot measure the complete traffic path. Drop attribution remains
  unresolved; no claim of Orishu packet loss is made.
- All workers and measured generators exited zero. Independent SSH audit
  found no owned worker, capacity TLS listener or client socket remaining;
  original staged worker/CLI hashes were unchanged. Local generators were gone
  and copied credentials/certificates were removed. Private run evidence and
  remote ephemeral state were preserved.

## Evidence and validation

Run `q-a1adeea782`, private evidence root `/tmp/orishu-pi-wired.UwRaXn`.
The command uses the [short-comparison recipe](../testing-worker-pi-cluster.md#off-host-authenticated-https-capacity-profile)
with the existing inventory/start policy, `--baseline-per-worker 5000`,
`--short-comparison`, `--max-elapsed-seconds 360`, and the unchanged native probe.
Private inventories were not staged or committed.

| Artifact | SHA-256 |
| --- | --- |
| Native probe | `97c07b640328839fb22d7d8f7be73680bc75af479e3e18aed8d0d77e64abb694` |
| Frozen coordinator/harness archive | `d2029cc786905f0bb2fc33d02ec29e812cb24b30ebedfc13b8ed11c7e2f3eb90` |
| `run/result.json`, including failed fourth window | `25251167ffa2040b8c16b87865d4e0a522eb6ca9dba1d775f7dba9bbee607ae6` |
| Independent cleanup audit | `e24a6671055ca1cd85384ea3996e45e3d92d777e45bfec27d175a2b06cf5c851` |
| Exact-source Pi route observations | `d10215a670ab69a5bf7ba94487ec2c4306a4af742b405dc76678c94bd33635ec` |
| Index including routes and all sixteen host-window files | `78933953d78e36d68f592430b9bf8da80b6f040c991903e2f3a13c6bd5140fde` |

151 Pi-harness tests and 36 existing telemetry tests passed. New tests cover
the exact six-window selection, stop-on-failure behavior and preservation of
the adaptive curve. Documentation and whitespace checks passed. No Rust source
changed this increment, so Rust tests/builds were not rerun.

## Next action, authority and trade-offs

To exclude Wi-Fi, first arrange verified Ethernet routing on the three affected
Pis, then check both directions with the exact source addresses before running
the unchanged short comparison. Options include preferring the wired route or
temporarily disabling Pi Wi-Fi; the latter can affect SSH/mDNS connectivity.
Network-policy changes require operator approval and a rollback/access plan;
this experiment did not authorize or perform them. Do not blindly disable the
interface carrying the current SSH session.

Separately, a repeated request failure needs bounded error-classification
evidence. Preserve the current stop gate and report incomplete evidence rather
than forcing a successful comparison. No scientific or M4 observability
acceptance criterion is changed by this diagnostic.
