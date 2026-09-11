# Prepare a three- to five-Raspberry-Pi formation experiment

Status: **four-Pi v2 pilot executed; rate, conditional timing and cleanup gates
met, with host-network findings and no Pi overhead acceptance result**. See the
[pilot checkpoint](measurements/formation-pi-pilot-2026-09-11.md).
The subsequent [sixteen-worker capacity diagnostic](measurements/formation-pi-capacity-2026-09-11.md)
records a highest observed aggregate 86.2k local-summary requests/sec with
colocated generators; it is not isolated worker capacity or overhead acceptance.
Owner: [physical-cluster experiment task](tasks/implement-physical-formation-experiment.md).
Context: [local 108-cell results](measurements/formation-post-diagnostic-2026-09-10.md).

The operator wants real dedicated machines and real peer networking, including
low-performance target configurations. A noisy laptop result does not block
preparing this experiment. Three Pis establish the physical baseline; five
extend it if available. Neither the one-worker-per-host baseline nor the
separate sixteen-worker diagnostic proves the thirty-worker performance target
or retroactively turns the local inconclusive curve into a pass.

## Publishing experiment evidence

Keep real inventories, raw route snapshots, endpoint-bearing receipts and
credentials private. Before committing a report, replace lab IPv4/IPv6
addresses with documentation-only addresses (for example `192.0.2.11`), real
hostnames with stable role aliases (`pi1`, `pi2`, etc.), and MAC-derived device
names with descriptive aliases. State that identities were anonymized, preserve
cross-report mappings and routing relationships, and never change measurements
or overwrite the private original evidence. The checked-in inventory example
must contain placeholders only. Root-level `pi.lab.*.json` inventories and the
local `update-fleet.sh` helper are Git-ignored; do not force-add them.

## Hardware and OS

Use the Pis already available; report each model and RAM size before choosing
the build/rate profile. Prefer a homogeneous group for the first paired
overhead experiment. If models differ, record each role and analyse them
separately rather than describing the group as identical hardware.

The first hardware checkpoint uses two Pi 5s and one Pi 4, all with 8 GB RAM,
Ubuntu 26.04 ARM64 and wired Gigabit Ethernet. This is a valid experiment
input, not a requirement to replace Ubuntu with Raspberry Pi OS. Retain
per-node baselines for the mixed models. Verify firmware/clock tooling on the
actual distribution; missing readings are not zeros.

- Same **64-bit Raspberry Pi OS Lite** image on all nodes, with the exact image
  date/checksum recorded. The current official OS is based on Debian Trixie;
  pin an image rather than repeatedly installing whatever is newest.
  [Official OS documentation](https://www.raspberrypi.com/documentation/computers/os.html)
  and [headless setup](https://www.raspberrypi.com/documentation/computers/getting-started.html)
  describe the available editions and Imager setup.
- Wired Ethernet through the same switch, stable DHCP reservations or static
  addresses, adequate cables and no deliberate traffic shaping. Record link
  speed/duplex/MTU; an older/slower Pi remains a valid named configuration.
- Suitable power supplies and cooling. Pi 5 guidance recommends its 27 W
  supply and active cooling for best performance. Avoid overclocking; record
  `vcgencmd get_throttled` before/after, including historical flags rather
  than clearing them silently. See [official hardware guidance](https://www.raspberrypi.com/documentation/computers/raspberry-pi.html).
- A separate coordinator, such as this development machine, reachable over
  SSH and the lab network. It stores evidence and can host telemetry tools;
  it is **not** an Orishu membership leader or simulation authority.
- Enough local space for artifacts/results. A native Rust build needs much
  more space/RAM than running the worker; use an ARM64 build host if small Pis
  cannot build comfortably. Do not build or update packages during measurement.

No Kubernetes, Docker/Podman, desktop environment, Kagami GUI, system-wide
profiling changes or passwordless root access is required for the baseline.

## Install once on each Pi

In Raspberry Pi Imager, create a dedicated normal user (examples use `orishu`),
set a unique hostname, enable SSH and install the coordinator's **public** SSH
key. Keep the private key on the coordinator or in its local SSH agent. The
[official SSH instructions](https://www.raspberrypi.com/documentation/computers/remote-access.html)
cover Imager key provisioning. Verify server host-key fingerprints using a
trusted console before recording them in the coordinator's `known_hosts`.
Do not disable host-key checking or upload passwords/private keys here.

On each Pi, run these administrator actions yourself once:

```sh
sudo apt-get update
sudo apt-get install --no-install-recommends openssh-server python3 ca-certificates openssl curl rsync iproute2 iputils-ping procps chrony ethtool
sudo systemctl enable --now ssh chrony
chronyc tracking
python3 --version
uname -m
vcgencmd get_throttled
```

Expect `aarch64` and Python 3.11 or newer. `vcgencmd` is supplied by Raspberry
Pi OS tooling; if missing, install the OS's matching firmware utilities before
measurement. Installing Chrony changes the time service and may replace an
existing time-sync package: inspect apt's proposed changes. We need bounded
clock uncertainty, not two competing time daemons. Package versions and
clock-sync evidence must be frozen/reported before the real run.

The preparation script does **not** execute apt, alter firewalls/governors,
configure SSH or start a worker. Review package/service commands before use.
Package names target Raspberry Pi OS/Debian; other distributions need an
explicit equivalent, not an assumed installation success.

## Run the per-node script

Copy `scripts/pi_lab_node.py` from the reviewed source snapshot to each Pi.
As the non-root experiment user, run:

```sh
python3 pi_lab_node.py prepare --root /home/orishu/orishu-lab
python3 pi_lab_node.py check --root /home/orishu/orishu-lab --interface eth0
```

Replace user/path/interface where necessary. `prepare` creates a **new**
private root with `bin`, `runs`, `results` and an ownership marker. It refuses
an existing root, symlinked path or parent owned by another account; it never
reformats or replaces existing data. Run `check` again for subsequent visits,
not `prepare`. Partial setup is retained on failure; inspect it rather than
deleting a broad home/workspace path.

`check` reports architecture/OS/kernel, RAM/disk, wired-link state, tools,
time synchronization, firmware temperature/throttle flags, governors and
artifact hashes/ELF architecture. It does not run supplied binaries, reveal
credentials, read worker state, open a network listener or change configuration.
Its initial missing-artifact warning is expected before staging binaries.
`infrastructure_ready` covers setup checks only; `experiment_ready` deliberately
remains false until the planned remote harness and hardware validation exist.
Warnings such as unknown synchronization or throttle flags still need review.

## Coordinator and SSH collection

Install Python 3.11+, OpenSSH client and rsync on the coordinator. The readiness
tools use Python's standard library only. Configure local SSH aliases:

```sshconfig
Host orishu-pi1
    HostName 192.0.2.11
    User orishu
    IdentityFile ~/.ssh/orishu_lab
    IdentitiesOnly yes
```

Add corresponding aliases for the remaining nodes. These documentation-range
addresses are placeholders: replace them with actual reachable lab addresses.
Keep host-key verification enabled. No SSH password, private key or unrestricted
sudo credential belongs in the JSON inventory.

Copy [the inventory example](../etc/experiments/pi-lab.example.json), replace
all addresses/aliases/paths, and add two uniquely named nodes for a five-Pi
experiment. Verify that each alias actually names a distinct physical machine;
unique strings alone cannot prove hardware identity. Then, on the coordinator:

```sh
python3 scripts/pi-lab.py validate --inventory /absolute/path/pi-lab.json
python3 scripts/pi-lab.py check --inventory /absolute/path/pi-lab.json --output /absolute/path/new-readiness-results
```

`validate` does not contact hosts. `check` streams the exact hashed per-node
script through key-only SSH (`BatchMode`, strict host keys, no forwarded agent
or inherited port forwards), with a 60-second/128-KiB cap per host. It stores
one private readiness JSON per responding node plus a summary, preserving
unreachable/invalid/not-ready outcomes. It attempts every listed node once,
without automatic retry. Raw SSH stderr is hashed rather than copied into
the summary. No remote files are installed or edited by `check`.

Once actual aliases are provided and SSH works from this machine, the assistant
can run this readiness collection. The later driver will use the same SSH
trust boundary for bounded experiment control and **sanitized** result fetching;
the performance driver is not implemented by these two commands. The separate
`smoke` action below implements bounded formation/recovery control.

## Run the real-network formation smoke

```sh
python3 scripts/pi-lab.py smoke --inventory /absolute/path/pi-lab.json --output /absolute/path/new-smoke-results --mode compiled_off
```

Choose `compiled_off`, `omitted`, or `metrics`. Run modes sequentially with
fresh output directories. This explicitly starts disposable workers; `check`
remains read-only. Reserve UDP 9000 on each selected peer IP and loopback TCP
9168 for metrics mode. An occupied port is a failure, never permission to stop
another process or alter a firewall.

The coordinator streams its reviewed helper and supervisor over strict SSH;
it does not implicitly execute a remote checkout's scripts. Each node snapshots
the selected worker and fixed CLI into a new private `runs/p-…/bin` directory,
verifies hashes, and creates its own state and Unix socket. Original staged
binaries, services, host policy and source checkout remain untouched. The
whole formation/recovery sequence has a 120-second deadline; node sessions
expire after 180 seconds, with an independent `timeout` watchdog and a
five-second forced-shutdown allowance. EOF/disconnection triggers local cleanup
too. Ubuntu's tested `timeout` is uutils; tests verify actual child termination,
not an assumed GNU-specific expiry status.

Allowlisted operations form A→B→C (or extend to five nodes), verify exact
identities/certificate bindings/liveness, propagate lock/unlock, then leave
and readmit the last node. Dead history remains: after a three-node rejoin,
expect four member records and three live members. Metrics mode also checks
the four loopback diagnostics routes after formation. This is correctness and
endpoint evidence, not overhead, trace-delivery, or disruption stress testing.
Five-node execution is not yet hardware-verified.

Results retain sanitized identities, stage timings, diagnostics receipts,
artifact hashes, failures and cleanup status. Join material travels only over
SSH, occupies a transient 0600 file, and is removed after its single join
submission. Worker state/operator credentials remain on their node: never
archive the remote run directory as a result bundle. Logs are continuously
drained with constant-memory size/hash accounting, not copied into results.
Private remote snapshots/state remain for deliberate later cleanup; the helper
never recursively deletes them. Inventory/output bundles are local lab data
and should not be committed.

## Software/artifacts for the actual experiment

The first updated ARM64 probe was separately staged on all four available Pis
as `bin/formation-telemetry-probe-node`; its frozen digest/build record is in
the [four-Pi probe checkpoint](measurements/formation-pi-probe-2026-09-11.md).
It predates the current input-2/output-5 timing profile. The old digest
`087e61e6af7e79f0afaf1e604742d91f7f1b190fa60f890362a74ff72868cbab`
does not identify a compatible v2 probe. A matching
[native v2 build](measurements/formation-pi-probe-v2-build-2026-09-11.md) is now
staged on all four Pis under **`bin/formation-telemetry-probe-node-v2`**, the
filename selected by the harness. Both prior probes remain unchanged. Native
startup and schema-1 rejection pass; v2 warmup/timing remain unverified. Set
`PI_PROBE_SHA256` to
`9e44762e7ae766881677fec865b23f7f76ed5025a3dc0e0a7913a16e0af1c9d3`
for this build, then verify warmup/formation without a timed window:

```sh
python3 scripts/pi-lab.py probe-preflight --inventory /absolute/path/pi-lab.json --output /absolute/path/new-probe-results --mode compiled_off --probe-sha256 "$PI_PROBE_SHA256"
```

This starts workers and makes 64 warmup requests per client (two clients per
node), then cancels each probe before its start byte. Nonzero EOF-cancellation
exits are retained; they are not load completion. The digest is specific to
this build, not a permanent expected hash for future source changes.

| Location | Required before measurement |
| --- | --- |
| Each Pi | Release ARM64 combined worker, feature-omitted worker, `orishuctl`, revised node-local load probe and bounded node supervisor; Python and OS tools above |
| Coordinator | New SSH experiment driver/reader; same source/configuration manifest; official Collector 0.160.0 and Prometheus/promtool 3.5.0 for the selected telemetry walkthrough, with architecture-specific checksums |
| Build host only | Rust toolchain pinned from this workspace (record exact `rustc -Vv`), Git, C/C++ compiler, CMake and pkg-config; no GUI build dependencies when building only worker/CLI/probe |

A conservative native ARM64 build environment can be prepared with:

```sh
sudo apt-get install --no-install-recommends git build-essential cmake pkg-config
```

Install Rust using reviewed [official Rust installation instructions](https://www.rust-lang.org/tools/install),
then use the supplied **frozen source snapshot including the current changes**.
Do not substitute `git archive HEAD` while the fixes are uncommitted, and do not
copy the present x86-64 executables to a Pi. Build both variants on a compatible
ARM64 userspace (or a validated matching cross-toolchain), before timing:

```sh
CARGO_BUILD_JOBS=2 cargo build --locked --release -p orishu-worker -p orishuctl --features orishu-worker/observability,orishu-worker/otlp-tracing --bins --example formation-telemetry-probe --target-dir target/pi-enabled
CARGO_BUILD_JOBS=2 cargo build --locked --release -p orishu-worker -p orishuctl --target-dir target/pi-omitted
```

Use fewer build jobs if required by RAM. Pin the exact compiler patch and
dependency lockfile; no `target-cpu=native` across different Pi models. These
are build recipes, not ARM64 build verification. Stage the four binaries under
the names listed by `pi_lab_node.py` (`orishu-worker-omitted` is the renamed
minimal worker). The new `load-node` seam below has now been rebuilt and
warmup-verified on ARM64 under the separate filename above; it is absent from
the operator's originally staged probe and still needs timed qualification.
Use **`target/pi-enabled/release/orishuctl`** for every mode. The CLI has no
worker telemetry feature switch; using one frozen CLI avoids build variation.
Cargo feature unification can affect dependency builds, so do not assume the
two target directories necessarily contain byte-identical CLIs.
For AWS-LC build requirements, see the [upstream platform guide](https://aws.github.io/aws-lc-rs/platform_support.html).

## Planned experiment flow and network policy

1. Validate readiness, source/artifact hashes, clock uncertainty, wired links,
   throttle/undervoltage state and available disk. Record power/cooling/OS and
   model per node. No installs, builds, upgrades or other tests during a cell.
2. Start one ordinary worker per Pi on its actual LAN peer address (e.g.
   `--listen.peers 192.0.2.11:9000`, replacing the example). Permit that chosen
   **UDP** port bidirectionally only among experiment Pis for QUIC peers.
   Permit SSH TCP 22 from the coordinator. Do not expose the operator API:
   keep it on each node's private Unix socket.
3. Form A→B→C (and D→E when selected) using real introducer material conveyed
   only over SSH/private files. Verify exact formation/node/fingerprint/live
   views and policy convergence using the production CLI. No public join
   material, state directories, key files or credentials in fetched evidence.
4. Each node supervisor runs a local typed-client load generator against its
   own Unix socket. **Peer traffic crosses Ethernet; this first profile does
   not measure an external client's network/TLS latency.** Client/scraper/
   supervisor CPU is reported separately from worker CPU on the small Pi.
5. Keep metric listeners on loopback and scrape locally. Prefer an off-Pi
   Collector on the coordinator to keep export processing off the measured
   nodes; select private-LAN OTLP HTTPS with collector-only mTLS credentials
   and firewall scope during harness implementation. Do not expose a plaintext
   unauthenticated public receiver or tunnel QUIC peers through SSH. Verify
   real causal receipts separately from counter-only timed collection.
6. Warm all load clients, arm a future start window using measured clock
   offsets/uncertainty, then use each node's monotonic clock for duration and
   CPU brackets. Record actual cross-host overlap and fail excessive skew;
   SSH command arrival is not synchronized start. No cross-host monotonic
   timestamp subtraction or inference of one-way trace latency from NTP alone.
7. Execute reviewed paired modes/rounds; retain all rate failures, errors,
   drops, temperature/undervoltage flags and incomplete cells. Measure CPU
   locally (remote PIDs cannot be sampled using coordinator `/proc`).
8. Stop only owned processes with a bounded local watchdog even if SSH drops.
   Fetch allowlisted, bounded JSON/log receipts with hashes; exclude credentials
   and runtime state. Preserve failure evidence without an automatic rerun.

The starting offer is 500 requests/s/worker with two clients and the existing
ten-second window. The recorded four-Pi pilot meets ≥99% delivery in one
compiled-off run; repeatability and paired overhead are not established.
If a later Pi experiment cannot sustain ≥99% delivery, retain
that failure and review a lower offered rate **prospectively**, as a differently
named experiment—not a pass at 500 requests/s or a relaxed loss gate.
Keep six modes/six rotated rounds and the below-10% normal CPU/p95 goals unless
the operator reviews a different physical-host plan. Hardware qualification
at four/five nodes, synchronization/deadlines, elapsed budget and source freeze require
their own explicit profile; no new acceptance batch starts from this guide.

## One-window pilot command — executed with retained findings

The operator approved and ran the proposed policy once. The
[four-Pi result](measurements/formation-pi-pilot-2026-09-11.md) meets rate and
conditional overlap gates, with clean shutdown, but retains rising host RX-drop
counters. This verifies the command's scoped hardware path, not a repeatability
claim or an overhead comparison. The recipe below requires approval for any
new attempt; the previous approval does not authorize retries.

`pilot` now integrates a pristine 3–5-node formation, exact membership checks,
warmup on every node, three fresh clock exchanges per ready probe, a planned
future start, one concurrent ten-second window, post-window membership checks
and bounded cleanup. It supports `compiled_off` and `omitted` only. It does not
run a comparison matrix, timed metrics/trace collection, leave/rejoin, retries,
builds, package installs or host-policy changes. `smoke` and `probe-preflight`
remain separate commands that do not run timed load.

Review the [proposed pilot policy](../etc/experiments/pi-pilot-policy.example.json)
and approve the separate five-minute allowance **before executing**:

```sh
python3 scripts/pi-lab.py pilot --inventory pi.lab.x4.json \
  --output /tmp/pi-pilot-new --mode compiled_off \
  --probe-sha256 "$PI_PROBE_SHA256" \
  --pilot-policy etc/experiments/pi-pilot-policy.example.json
```

Use a fresh output directory and the verified v2 probe digest documented above.
Invoking this command starts workers and fixed-rate traffic on the inventory's
machines; it is not a readiness check. The policy file is a proposal, not a
record of operator approval. It requires `schema_version: 1`,
`max_elapsed_seconds: 300`, and the complete `start` and `overlap` policies
described below. Unknown/duplicate fields and files over 4096 bytes are rejected.
Malformed policy, unsupported mode and missing probe digest fail before SSH.

The proposed settings use an eight-second lead, clock round trips up to 25 ms
(versus 3.1–11.3 ms in the earlier read-only checkpoint), and an assumed relative
drift bound of 500 ppm. Post-window analysis allows up to 80 ms conservative
start skew and requires at least 9.9 seconds of common probe window. These are
diagnostic measurement bounds, **not** new CPU/p95 overhead acceptance gates.
The full proposed assumptions and client-activity limits are in the file;
observed endpoints cannot establish that a clock never stepped between reads.
The collection-span limit includes the future-start wait and control traffic,
not just the ten-second offer. The recorded pilot met its conditional timing
bounds; unobserved clock behavior and repeatability remain outside that claim.

Setup, warmup, scheduling, measurement and the final membership check share
the original 120-second work deadline; stages never restart it. Collection
can only shorten session deadlines to its 45-second ceiling. Cleanup has a
separate bounded stop/SSH-close allowance and node-owned 180-second watchdogs;
the result records elapsed time against the separate 300-second pilot limit.
On a lost reply, a dispatch attempt is retained as potentially executed, not
retried or falsely reported as an unstarted window.

Private `result.json` records source/bundle hashes, policy, fresh clocks,
identities, attempts, findings and cleanup receipts. It links the digest of
`window/collection.json`, whose manifest and per-node files retain raw
measurements, environment and timing reviews. Exit 0 / `complete_unqualified`
means the pilot completed without reported pilot findings, **not** M4 or
overhead acceptance. Exit 2 means `complete_with_findings` or `incomplete`;
failed rate/timing/environment gates retain their evidence without retries.
Unverified cleanup prevents a successful result. Inspect the findings before
planning a comparison; do not treat a failed pilot as permission to change
the workload or repeat it automatically.

## Four hosts / sixteen workers: capacity diagnostic

This heading preserves the historical experiment link. The current harness
also accepts three to five hosts and `--workers-per-host 1` or `4` (default).
Use the [post-M4 five-Pi plan](measurements/formation-post-m4-five-pi-plan.md)
for the new 5-/20-worker comparison and its separate approval/budget.

The [2026-09-11 hardware report](measurements/formation-pi-capacity-2026-09-11.md)
records seven completed cells, all earlier attempts and independently verified
cleanup. The highest observed aggregate is 86.2k requests/sec at eight clients
per worker; the generator itself uses nearly half of each host's CPU.

The original `scripts/pi-lab-capacity.py` diagnostic used four physical
hosts from the inventory, with four worker processes per host. Do not
duplicate inventory entries: physical hosts and membership nodes are different
dimensions. Each process has a private state/client socket and a distinct peer
port, 9000–9003. All sixteen workers join one formation over real QUIC peers.
Four processes do not imply four runtime threads or automatic core pinning;
the diagnostic retains actual process thread counts without changing affinity,
governors, firmware, services or worker binaries.
The observed default is four Tokio executor threads plus the main thread per
Pi worker, not single-threaded async IO. The
[post-PoC executor-sizing review](tasks/review-worker-executor-performance.md)
tracks oversubscription and alternative policies. The new optional
`--executor-threads 1|2|4` sets `TOKIO_WORKER_THREADS` only for disposable
experiment workers; omission clears inherited overrides and retains the worker
default. It does not change a host policy or select a production default.
These are Tokio multi-thread pools, not the current-thread runtime.

`--placement` opts into the inventory interface for peer sockets and, with
`--network-probe`, TCP client listeners/replies. The launcher uses wildcard
binds plus the explicit advertised peer address as required by ADR 0026.
Unix administration and loopback diagnostics are unchanged. Kernel socket and
device-counter receipts remain private. No route, firewall or link is changed.

This is **colocated public cluster-summary API capacity over Unix sockets**,
not peer-message throughput, remote-client networking, simulation throughput,
or observability-overhead acceptance. One two-executor-thread Rust probe per
host serves its four workers. Worker, probe and supervisor CPU remain separate;
the generator competes for the same four cores and can limit measured capacity.
Mixed Pi models must retain per-host/per-worker results, not just a cluster sum.

Build the probe example with the pinned toolchain and stage the resulting ARM64
binary under the new, no-overwrite name
`bin/formation-telemetry-probe-capacity-v3` for current invocations. Preserve prior probes and record
the source archive, compiler, features and all binary digests. A scoped command
for an explicitly authorized diagnostic batch is:

```sh
python3 scripts/pi-lab-capacity.py \
  --inventory /absolute/private/pi-lab.json \
  --output /absolute/private/new-capacity-results \
  --probe-sha256 ACTUAL_CAPACITY_PROBE_SHA256 \
  --baseline-per-worker 5000 \
  --max-elapsed-seconds 600 \
  --start-policy etc/experiments/pi-pilot-policy.example.json
```

Rebuild the coordinator's `--network-probe` too when using HTTPS: new network
input/output schema 4 and Unix schema 2 explicitly carry `global_workers`.
Network schema 4 also requires bounded client-error details; network schema 3
retains its earlier explicit-topology report without those details.
Old network schema 2 / Unix schema 1 retain their exact 16-worker shape for
historical replay, but old probe executables cannot execute the new profiles.
The CLI defaults to four workers per host; five inventory entries therefore
mean 20 workers and a 100,000/sec aggregate offered baseline. Never confuse
physical hosts, worker processes and executor threads.

Here 5,000 means **per worker**; multiply by the selected worker count (80,000
cluster-wide for the historical four-host/four-worker setup).
It is now the default; `--baseline-per-worker` explicitly overrides it within
100–100,000. This capacity target does not change the existing 500/second
observability acceptance workload.
Windows last ten seconds. The baseline uses 32 bounded, persistent clients per
worker. If every worker completes at least 99% of its offered arrivals, the
rate doubles up to three times; otherwise it halves up to three times to bracket
deliverability. Missed arrival slots are counted and never queued/replayed.
Timer granularity and generator scheduling delay are part of this profile.
Next, unpaced windows use 2/8/32 clients per worker. If 32 clients improve
aggregate throughput over eight by more than 5%, try 64, the existing worker's
connection ceiling; then confirm the best observed concurrency once. Each successful response
must retain the expected formation, source node, configured live-member count, unlocked
policy and introducer readiness. Failed cells are retained, not silently retried.

The coordinator keeps one whole-run deadline and cleanup reserve; node-owned
worker watchdogs cap lifetime at 600 seconds and probe watchdogs at 60 seconds.
Request/identity failures, sample-cap hits, unavailable/unsafe thermal evidence,
nonzero firmware flags, swap, or failed membership checks stop further escalation.
The conservative sampled-temperature stop is 75°C, a lab policy rather than a
claimed hardware damage threshold. Process resources are sampled at 100 ms and
host environment at one second. Missed samples remain findings, not zero cost.
Only run-owned processes are stopped; private run directories remain as evidence.

The start policy supplies offset-aware scheduling. Raw pre/post clocks and
probe-owned timestamps are retained. Capacity analysis uses an explicit 20 ms
offset-change allowance and requires a conditional nominal start-skew bound
below 100 ms; it does not claim continuously verified clock accuracy or client
activity. This is distinct from the one-worker pilot's overlap acceptance
contract. Reproducing a command requires a new output directory and an appropriate
experimental allowance; neither the old pilot nor this example grants it.

### Off-host authenticated HTTPS capacity profile

The [first hardware run](measurements/formation-pi-network-capacity-2026-09-11.md)
completed nine windows with independent cleanup. It peaked at 36.75k summary
requests/sec through the desktop's Wi-Fi route; this is not an all-wired ceiling.

Add `--network-probe /absolute/path/to/native/formation-telemetry-probe` to
the capacity command. Build that example in release mode on the **coordinator**
with the pinned toolchain and `observability,otlp-tracing`; the supplied
`--probe-sha256` now identifies this local native binary, not the ARM64 probe.
One coordinator probe process per physical host (two executor threads each)
drives that host's selected one or four workers. No generator is installed or
run on the Pis in this mode.

The adapter creates disposable TLS client listeners on each inventory IP at
9440–9443, alongside the private Unix admin sockets and QUIC peer ports
9000–9003. OpenSSL must already be installed on the Pis. Fresh certificates
include the exact IP SAN and are trusted through verified SSH; TLS private keys
stay on their Pi. Worker-local operator credentials travel only over the SSH
control channel into mode-0600 temporary coordinator files, never arguments,
environment variables or published result JSON. These copied credentials and
certificates are removed at cleanup. Existing worker state and staged binaries
are untouched; ephemeral node state remains private evidence.

Before load, every endpoint must reject missing/wrong bearer tokens, untrusted
certificates and a wrong server identity. Typed-client warmup must then succeed
with the correct credential and exact configured membership. There is no
plaintext, insecure-certificate or SSH-forwarded-socket fallback.

Current network load/output uses schema 4 and network-specific kinds, separate
from the colocated schema 2 profile; the older fixed-sixteen shapes remain for
historical replay. Pi receipts contain the selected local workers and their
supervisor; coordinator receipts add a separately measured generator, CPU and
temperature observations. Retain both actual generator starts and offset-mapped
Pi sampler starts; require their conditional joint skew below 100 ms. This
does not prove continuous per-client activity or a paired overhead result.
Report desktop routing/link type and generator pressure: moving load off the
Pis can expose the coordinator's network or CPU limit rather than the cluster's.

Per-thread scheduler snapshots are collected during the existing pre-start lead
and after the resource window, with explicit read timestamps and a wider-bracket
scope marker. They include idle lead time; do not interpret raw deltas as exactly
ten seconds of load or substitute them for CPU/resource-window measurements.
Collection that overruns the lead aborts that local measurement path; other
already-dispatched samplers/generators may have started, so retain partial
receipts and never infer a globally idle window from that error. This
ordering avoids diagnostic reads delaying the scheduled sampler start, as seen
in the [five-Pi timing diagnostic](measurements/formation-post-m4-five-pi-plan.md).
The same round passed physical placement/recovery after an explicitly approved
temporary ARP correction, but stopped capacity load on unclassified client
errors. Device binding does not repair a neighbour's wrong-interface ARP mapping;
verify ingress and saved-setting rollback before changing host policy.

For an explicitly approved transport comparison, add `--short-comparison`.
This network-only selection runs `(baseline, 32 clients)`, `(unpaced, 32)`,
`(unpaced, 64)` in that order, then repeats the three profiles once: six
ten-second windows, without adaptive rate stepping. All identity, security,
timing, resource and cleanup gates are unchanged. Rate-delivery misses remain
findings; request/identity/safety failures still stop the comparison. A
`--max-elapsed-seconds 360` ceiling retains setup and cleanup headroom; it is
not six minutes of measured traffic. Use the same hashed probe and workers
when comparing a previous Wi-Fi run with Ethernet, and verify actual routes
before and after rather than inferring the path from a plugged-in cable.
The [first wired-desktop comparison](measurements/formation-pi-wired-comparison-2026-09-11.md)
stopped on four request errors after four windows; it also found that three
Pis still selected `wlan0` for replies from their Ethernet server IPs. Before
claiming all-wired operation, check each Pi's route to the generator **from the
bound server IP**, as well as the coordinator route and link negotiation.
An inventory IP assigned to `eth0` does not guarantee egress on `eth0`.
Interface-scoped counters cannot measure traffic using another interface.

For an explicitly approved error-classification diagnostic, `--error-diagnostic`
selects **one** unpaced 32/64-client pair, without a baseline, repetition or rate
sweep. It requires the network profile and cannot be combined with
`--short-comparison`. The first failed window stops the pair. Network schema 4
requires `client_error_details`: bounded fixed-category/status counts summing
exactly to `transport_errors`, and first-error request/completion/duration times
in microseconds relative to the probe's start. Each client stops on its first
error, so counts cannot exceed client concurrency. Empty counts require a null
first error. Unknown fields/categories, duplicate buckets and inconsistent
counts/timing are rejected. Raw messages, response bodies, URLs and credentials
are never exported. Older network schemas 2/3 retain their original row shape.
Statuses are those retained by `ClientError`, not necessarily observed HTTP
statuses: its API-error variant can hold zero or a synthesized status. Known
client-owned empty-body formatting retains its HTTP status explicitly; other
transport failures remain grouped rather than guessed to be timeouts. See the
[five-Pi diagnostic](measurements/formation-pi-client-errors-2026-09-11.md).

The network capacity profile also accepts `--ssh-via-peer-address`: connect
the control channel to the inventory's literal `peer_address`, but retain
`ssh_host` as `HostKeyAlias` with strict existing host-key verification. This
avoids mDNS choosing a different NIC without disabling trust checks or changing
the inventory. It selects a destination address, not an OS device; verify
actual routes for control and load independently. The default remains hostname
SSH. Result provenance records this selection.
The IP destination must have the required SSH account/key/port configuration;
`HostKeyAlias` preserves host-key lookup, not the hostname's entire SSH config.
Clock inputs are now saved in
`cell-N-clock-evidence.json` **before** start-policy validation, including on
rejection; do not weaken a clock limit to get a run through.

The [route-corrected comparison](measurements/formation-pi-ethernet-routing-2026-09-11.md)
retains six windows and rollback evidence. With temporary routing, arm bounded
rollback before mutation and record its actual execution relative to the last
sample. Capture both NICs; background Wi-Fi packets do not by themselves imply
that HTTPS load used Wi-Fi.

### Worker-enforced interface placement

The worker can now bind each role to one named interface instead of relying on a
temporary route: `--interface.peers eth0` and `--interface.clients eth0`, decided
in [ADR 0026](adr/0026-worker-network-interface-placement.md). A placed role
needs a wildcard bind (`--listen.peers 0.0.0.0:6655` with `--advertise.peers
<inventory IP>:6655`), because a concrete address paired with a device binds
successfully and then matches no ingress. This is the intended replacement for
the manual `ip route` correction; re-running the network profile with these
flags, rather than with a temporary metric-50 route, is outstanding work.

The [seven-case isolated namespace proof](measurements/worker-network-placement-2026-09-11.md)
now verifies the Linux socket paths, including different peer/client devices and
fresh state propagation after link restoration. The report includes a rootless
`unshare` recipe requiring no host route changes. This is not a Pi recheck:
rebuild/deploy the placement-enabled binaries before running that physical
follow-up; the existing fleet and private inventories have not been changed by
the namespace verification.

Troubleshooting, in this order:

- **No `placement` lines on the worker's stderr.** The selection never reached
  the worker. YAML `spec.interface` is strict about its own keys, but a
  misspelled outer key such as `spec.interfaces` is discarded silently.
- **Startup exits 2 naming the interface.** The device does not exist in that
  network namespace, or a concrete bind was paired with it. Both are refused
  deliberately; the worker never falls back to an unconstrained socket.
- **Placement reported, but traffic still absent.** Check `ip route get <dst>
  from <bound source IP>` on both ends. A placed worker excludes the wrong
  interface, so a peer that still prefers the wrong route cannot reach it — the
  correct outcome, and the reason both ends must be placed or routed together.
- **Traffic stopped after a link event.** A socket bound to a device with no
  usable route fails route lookup rather than selecting another device.
  Recovery is the interface returning, or a restart. There is no hot reload.

Rollback is removing the two flags and restarting; nothing persists on the host,
and no routing table or firewall rule is written by the worker. Bounded
multi-interface lists, failover and pod-namespace semantics remain
[planned work](tasks/implement-worker-network-placement.md).

## Current verification and next handoff

The probe now accepts `load-node CONFIG` with a bounded tooling configuration:

```json
{"schema_version":2,"workers":3,"role":0,"target":{"socket":"/private/run/api.sock","formation":"exact-formation-id","node":"exact-node-id"}}
```

`workers` is the global count (3 through 5), `role` is zero-based and below that
count, and `target` is exactly one local Unix endpoint. It reuses public typed
clients, two clients per worker, preallocated samples, 64 warmup requests per
client and the ten-second fixed 500-request/s offer. Its READY/G/END/A handshake
matches the local probe, but output is now schema 5 with arrival profile
`fixed_500_per_worker_physical_pilot_v2`, `global_workers` and the global role.
Input schema 2 fails fast on an older staged probe. This is a tooling-profile
revision, not a worker protocol change. The previous ARM64 warmup checkpoint
used input 1/output 4 and **does not qualify the updated profile**. The subsequent
v2 build/staging checkpoint qualifies native startup only. The later
[approved pilot](measurements/formation-pi-pilot-2026-09-11.md) adds warmup and
one timed-window result, with retained environment findings. Existing
worker/CLI binaries and read-only clock/environment checks are unaffected.

The new report retains the probe's own shared arrival-window start and END
marker as bracketed wall-clock reads, the monotonic END-marker elapsed time,
and each client's first request, last timed completion and timed request count.
Activity offsets are relative to that shared start; clients that issue nothing
retain null activity, not a fictitious zero-latency request. The reader checks
client indices, totals, timing bounds and containment within the supervisor's
CPU/control bracket. Clock-step evidence remains an interval, not a zeroed
offset. These fields now support conditional overlap analysis; neither the nominal offer
window nor first/last endpoints prove uninterrupted traffic between them.
Existing local 3/10/30 profiles retain schemas 2/3. Node-local preparation,
resource sampling and a future-start window are implemented behind the SSH
session, with conditional timing evidence from the pilot:
SSH arrival is not synchronized start, and
the existing local acceptance summarizer does not accept schema 5. Load qualification
needs a fresh formation; after rejoin, retained dead history deliberately
fails the pristine membership-count predicate.

The physical **single-node** reader is available separately from the legacy
acceptance summarizer. It accepts the sampler's schema-2 envelope containing
the schema-5 load report, checks the expected role/member/formation/private
run socket and preserves separate process CPU, memory and timing accounting.
The coordinator applies it to `measure` responses and checks the requested
start window. For an already collected report, use:

```sh
python3 scripts/pi_lab_results.py --report /path/to/measurement.json \
  --expected-config /path/to/coordinator-load-config.json \
  --sha256 "$PI_MEASUREMENT_SHA256"
```

Supply the configuration from the trusted coordinator run manifest and the
digest from the collection receipt, not values copied from an unverified
report. The reader performs no SSH or remote file collection. Exit 0 means a
valid receipt (`status: valid_unqualified`), **not** acceptable performance;
rate/sample failures are retained under `measurement_issues` and distributed
qualification remains absent. Malformed, unavailable or digest-mismatched
input exits 1 with a sanitized error class. Cross-mode acceptance analysis
remains unfinished.

An internal coordinator `collect_window` operation can now dispatch to 3–5
already prepared nodes concurrently. It requires explicit per-node starts
from its caller; it does not choose a clock policy. The `pilot` command above
owns that orchestration. Before dispatch it checks unique roles/hosts/membership identities,
one formation/run/mode and identical worker/CLI/probe artifact hashes. Each
node gets one measurement attempt, including after an uncertain write. It
retains private per-node measurements, digests, reviews, before/after clock
exchanges and sanitized failures under a new output directory. A failure does
not discard other nodes' results or trigger a retry. Existing deadlines are
only shortened, with a 45-second collection ceiling.

`complete_unqualified` means collection completed, not that performance,
cross-host overlap or cleanup passed. Rate/sampling failures remain in the
per-node reviews. The caller must own the experimental allowance, select
reviewed clock-derived starts and stop every worker in a `finally` block;
collection records `cleanup_verified: false`; the outer pilot records its own
cleanup result. The recorded pilot verifies one hardware execution under
conditional clock/overlap bounds, not general synchronization guarantees.

The internal operation optionally accepts `timing_policy` for a locally tested
actual-window review. It revalidates raw measurements and both clock exchanges
per role, uses the full offset envelopes plus an explicit whole-collection
change allowance, and computes conservative start-skew and common-window
bounds. It uses probe-owned starts, not requested deadlines, and subtracts
END-marker lateness rather than counting it as extra traffic. Client first
requests and last completions have separate checks; they do not prove
uninterrupted activity. No measurements are pooled across Pi models.

The policy requires all nine integer fields below; no values are implicitly
approved. These are offline analysis limits, separate from the start-proposal
policy and experimental time allowance:

| Field | Meaning |
| --- | --- |
| `max_round_trip_ns` | Maximum clock-exchange round-trip upper bound |
| `max_offset_width_ns` | Maximum individual exchange offset-envelope width |
| `max_wall_change_ns` | Assumed maximum wall/monotonic mapping change over **any** subinterval; checked against sampled coordinator, node and probe brackets |
| `max_offset_change_ns` | Assumed maximum node-minus-coordinator offset change throughout the entire collection |
| `max_collection_span_ns` | Maximum whole-cohort clock bracket, positive and no more than 45 seconds |
| `max_pairwise_start_skew_ns` | Maximum conservative actual-probe start skew |
| `min_common_window_ns` | Minimum conservative shared window, positive and no more than ten seconds |
| `max_first_request_offset_ns` | Latest permitted first request from each client relative to its probe start |
| `min_last_completion_offset_ns` | Earliest permitted last timed completion from each client relative to its probe start |

Monotonic wall clocks and the whole-interval change limits are explicit
assumptions: endpoint samples can contradict them but cannot prove the absence
of an intervening step and reversal. Clock-read uncertainty is widened, never
rounded inward. Results say `within_conditional_bounds` or
`outside_conditional_bounds`, always with `clock_uncertainty_qualified: false`
and `acceptance_run: false`. Failed limits retain the per-node raw files and
numeric diagnostics; incomplete collection leaves the timing review
`invalid_or_unavailable`. Rate, environment and cleanup qualification remain
separate. The recorded pilot exercised this path with no timing-limit findings;
its whole-interval clock assumptions remain explicitly conditional.

After an RPC timeout, interrupted write or invalid reply, the coordinator
does not reuse that SSH session—even for `stop`. A late reply could otherwise
be mistaken for a different operation's acknowledgement. It closes the stream
and reports `clean: false, reason: session_reply_unverified`; EOF/signal cleanup
and the independent node watchdog remain the termination mechanisms. This is
not proof of an orphan or proof of clean worker exit: retain the failed attempt
and inspect its node cleanup evidence before declaring cleanup verified. No
automatic restart or measurement retry is authorized by this fallback.

The collection seam includes pre/post host-environment snapshots, sampled
outside the load/CPU window so firmware subprocess CPU is not hidden in worker
accounting. It reads the selected NIC's byte/error/drop counters, checks boot
and interface identity, and records CPU temperature plus raw firmware throttle
flags. It rejects unknown fields and invalid counter data; absent temperature
or firmware tooling remains unavailable rather than zero. Counter reset,
interface replacement and reboot yield no comparable network delta. Existing
historical drops are not attributed to a new run.

These NIC deltas include SSH/background traffic and the wider pre/post bracket;
do not divide them by the ten-second load duration or call them worker-only
throughput. Endpoint temperatures are not a continuous peak. The read-only snapshot command
path passed on all four supplied Pis with no workers started; that evidence
does not qualify under-load behavior.

The node-local measurement loop now also samples the selected interface and
CPU temperature at most once in each one-second slot of the ten-second window.
It performs bounded kernel-file reads only: no `vcgencmd` subprocess is spawned
inside the CPU window. Firmware flags remain the separate pre/post evidence.
The sample work is included in **supervisor** CPU, not charged to worker CPU;
it can still perturb the host and its overhead needs hardware measurement.
Delayed slots are skipped, never replayed as a catch-up burst. Each attempted
sample retains its read bracket or sanitized failure, and interrupted windows
retain the partial series in `measurement-incomplete.json`.

The raw `environment_series` is required in measurement envelope version 2;
the reader rejects version 1 rather than treating missing sensors as zero.
The Rust probe's input-2/output-5 contract and staged binaries are unchanged.
The series is bound to the private run, and collection additionally checks its
interface against the trusted inventory. The review exposes missed/failed
samples, a **sampled** temperature maximum, and a host-network delta only when
all ten snapshots have consistent identities and non-regressing counters.
The delta's first/last read-time bracket is recorded separately: it spans
roughly nine seconds, not the complete ten-second load and not worker-only
traffic. Missing temperatures remain unavailable, and peaks between the
one-second samples can still be missed. The pilot retained all ten snapshots
per node and exposed RX-drop findings; neither sampling nor its timing review
independently produces an acceptance pass.

`probe-preflight` now also records three authenticated clock exchanges per
node before probe warmup. Each receipt echoes the run ID and a fresh nonce;
the coordinator retains raw wall timestamps, monotonic read brackets,
round-trip bounds and the full possible node-minus-coordinator offset range.
These are exchange-time diagnostics, not a midpoint estimate or proof of
future alignment. The range assumes no coordinator wall-clock step during
that exchange; monotonic-versus-wall change evidence is retained separately.
Clock slews/steps on either host and drift between exchanges still require
before/after-window qualification. The explicit-limit assessment helper has
no default acceptance thresholds and always leaves window qualification false.
The recorded four-Pi warmup predates these exchanges. A subsequent
[read-only clock check](measurements/formation-pi-clock-2026-09-11.md) now verifies
the clock RPC on all four Pis, independently of worker formation or load:

```sh
python3 scripts/pi-lab.py clock-check --inventory pi.lab.x4.json \
  --output /tmp/pi-clock-check-new
```

Use a new output directory. This command opens clock-only sessions, with no
lab root/artifact access, worker creation, package installation or clock
modification. It captures three exchanges per host and cannot qualify a load
window. The observed Pi-to-coordinator offsets were around +50 ms, with
individual round trips of 3.1–11.3 ms. Scheduling still needs a reviewed
uncertainty policy; do not assume either clock is an external time authority.

`clock-check --start-policy /path/to/policy.json` optionally adds an
offset-compensated **start proposal**, never a load dispatch. The JSON object
must contain every field below, with integer values and no extra fields.
There are no implicit accepted defaults:

| Policy field | Unit / meaning |
| --- | --- |
| `lead_ns` | Nanoseconds ahead of the proposal's coordinator timestamp; 3–15 seconds |
| `max_age_ns` | Maximum age of every exchange; positive, at most 60 seconds |
| `max_round_trip_ns` | Maximum exchange round-trip upper bound |
| `max_offset_width_ns` | Maximum width of each raw offset envelope |
| `max_wall_change_ns` | Allowed coordinator wall/monotonic mapping deviation during the exchange and since receipt |
| `max_relative_drift_ppm` | Assumed bound on offset change per elapsed coordinator wall time, without clock steps; 0–1,000,000 |
| `max_start_uncertainty_ns` | Maximum per-node uncertainty after widening for age and assumed drift |
| `max_local_start_lateness_ns` | Assumed wakeup lateness bound, to be checked during a real run |
| `max_pairwise_start_skew_ns` | Maximum conditional spread including uncertainty and lateness |

All three exchanges per role must satisfy the policy and remain mutually
consistent after widening. The proposal uses their conservative union extent,
not the fastest sample or a narrowed intersection. A Pi clock ahead of the
coordinator receives a later Unix deadline to target the same reference
instant. Drift margins round outward in integer nanoseconds. Replayed,
reordered, stale, future or wrong-run evidence is rejected.

A proposal records its assumptions and remains `proposed_unqualified`, with
`dispatch_authorized: false`. Its deadlines soon expire: do not copy them into
a later worker run. A real pilot must use fresh evidence from its own prepared
sessions and verify wakeup/overlap afterward. If a requested proposal fails,
the CLI exits 2 while retaining successfully collected clock evidence and a
sanitized proposal error; it does not repeat SSH or relax limits. This proposal
path has local coverage only and selects no operator policy or experiment budget.

On 2026-09-11, 12 experiment-harness tests, 14 readiness tests, 36 existing
formation-harness tests and nine Rust probe tests passed. New tests include a
real Unix-socket/child/EOF-cleanup path, requiring local socket permission:
the constrained sandbox denies that test, while the normal-host run passes.
The subsequent four-Pi increment adds ten sampler tests and extends the
experiment-harness suite to 14 tests. Targeted probe Clippy passes. Run the new tests with
`python3 scripts/test_pi_lab_experiment.py`.
Sampler contracts run with `python3 scripts/test_pi_lab_measurement.py`.
The subsequent clock-evidence increment passes nine clock tests and extends
the session suite to 15 tests, including partial-batch retention and cleanup
after a mismatched clock receipt. Run `python3 scripts/test_pi_lab_clock.py`.
The single-node reader adds nine tests (including actual CLI/digest checking,
wrong-run/role/window rejection and sampler-to-reader composition in the
existing sampler suite). Run `python3 scripts/test_pi_lab_results.py`.
The collection increment adds nine tests for concurrent 3–5-node dispatch,
artifact/identity rejection, one-shot requests, partial clock/IO failures,
private result files and the real serialized coordinator/node request paths.
Run `python3 scripts/test_pi_lab_collection.py`. These use prepared-load test
fixtures, not real Pi load or a measured performance result.
Seven environment tests cover bounded kernel/firmware input, unavailable
instruments, historical versus new counter changes, resets/reboots, identity
changes during sampling and the node request allowlist. Run
`python3 scripts/test_pi_lab_environment.py`.
Five session-recovery regressions run with
`python3 scripts/test_pi_lab_session_recovery.py`. Their real pipe/framing tests
cover late/partial replies after timeout, a longer-timeout control, uncertain
writes and the unaffected healthy cleanup path.
Five clock-only session/collection tests run with
`python3 scripts/test_pi_lab_clock_check.py`, including the real streamed
bootstrap and refusal of all worker/filesystem operations.
The start-planning increment adds eight pure planner tests and extends the
clock-check suite to seven tests. Run `python3 scripts/test_pi_lab_start_plan.py`
for offset direction, stale/replayed evidence, drift rounding, interval
consistency, conservative sample retention and conditional skew limits.
The physical timing revision adds three sampler/reader tests and two Rust
probe tests. The focused suites now total 132 Python tests and 11 Rust probe
tests. Legacy local load schemas 2/3 remain separate; no new local curve or
physical timed run was performed to establish performance equivalence.

The conditional-overlap increment adds ten pure tests and three collection
tests, including the serialized coordinator/node path. The latest local run
passes 109 Pi-harness tests plus 36 legacy formation-harness tests (145 total).
Run `python3 -m unittest discover -s scripts -p 'test_pi_lab*.py'` and
`python3 scripts/test_formation_telemetry.py`; the Unix-socket fixture needs
normal local socket permission. Documentation and diff checks pass. No Rust
source or staged Pi binary changed in this coordinator/reader increment;
the earlier Rust validation checkpoint is retained, not claimed as a new run.

The periodic-environment increment adds seven kernel-series tests and two
sampler/collection tests. All 118 Pi-harness tests and 36 legacy formation
tests pass (154 total), including the local Unix-socket cleanup fixture with
normal socket permission. Coverage includes transient sampled temperature,
missed/failed reads, counter/identity changes, no firmware subprocesses, wrong
inventory interfaces and retained partial series. Documentation and diff
checks pass. No Pi load, Rust build or host-policy change ran for this increment.

The public-pilot increment adds twelve orchestration/CLI tests. All 130
Pi-harness tests and 36 legacy formation tests pass (166 total), as do
documentation and diff checks. The new tests use local
worker/clock doubles around the real formation, clock planner, concurrent
collector and reader; they do not claim real SSH or Pi execution. Run
`python3 scripts/test_pi_lab_pilot.py`. They cover 3–5 nodes, one-shot dispatch,
fresh clocks after all warmups, failed preparation/collection/cleanup, retained
findings, unchanged deadlines and explicit CLI policy requirements.

Local helper tests cover 3–5-node inventory bounds, duplicates/injection,
strict SSH options/quoting, private create-only preparation, ELF/hash checks,
bounded child IO/deadlines and retained per-host failure/mismatch outcomes.
Verification on 2026-09-10: all 14 helper tests and 36 existing formation
harness tests passed, along with formatting, diff and documentation checks.
Reproduce the helper checks with `python3 scripts/test_pi_lab.py` and the
existing harness checks with `python3 scripts/test_formation_telemetry.py`.
Those local tests alone do not prove hardware behavior. The separate
[three-Pi checkpoint](measurements/formation-pi-smoke-2026-09-11.md) now proves
scoped SSH/worker/CLI/formation and endpoint behavior. The later
[four-Pi checkpoint](measurements/formation-pi-probe-2026-09-11.md) verifies
updated ARM64 probe warmup too, but not timed sampling, continuous thermal
instrumentation or overhead. The readiness scripts are useful now;
the [implementation task](tasks/implement-physical-formation-experiment.md)
tracks the missing experiment driver without claiming it is delivered.

User inputs when ready: Pi model/RAM per role, chosen OS image, 3–5 nodes,
SSH aliases/usernames and lab IP/interface details, coordinator address and a
future experiment time budget. No credentials should be pasted into chat.

Four-node inventories are supported too: the physical tooling accepts the
bounded range 3–5, not only the original three/five-node sampling points.
The five-node cap keeps the existing command, reply, cleanup and membership
history bounds intact; broader sizes need a separately bounded profile. A
fourth Pi 4 Model B Rev 1.1 with 4 GB RAM was supplied after the first hardware
checkpoint. Keep that configuration and its per-node baseline distinct from
the three-node results; inventory acceptance is not hardware qualification.
The four-node read-only preflight passed on 2026-09-11: the added Pi reports
ARM64, Gigabit full-duplex Ethernet/MTU 1500, synchronized time, 44.8°C and
`throttled=0x0`, with the same four staged artifact hashes as the original
nodes. These are spot readings, not under-load evidence. The subsequent
v1 four-node formation/recovery and updated probe warmup checkpoint passed
without a timed measurement. The later
[v2 timed pilot](measurements/formation-pi-pilot-2026-09-11.md) met its delivery
and conditional timing gates, with retained host-network counter findings;
paired instrumentation-overhead acceptance remains open.
