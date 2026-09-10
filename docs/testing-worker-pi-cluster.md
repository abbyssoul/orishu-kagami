# Prepare a three- or five-Raspberry-Pi formation experiment

Status: **preparation/readiness scripts implemented; remote performance driver
planned; no Pi hardware or cross-host performance verified yet**.
Owner: [physical-cluster experiment task](tasks/implement-physical-formation-experiment.md).
Context: [local 108-cell results](measurements/formation-post-diagnostic-2026-09-10.md).

The operator wants real dedicated machines and real peer networking, including
low-performance target configurations. A noisy laptop result does not block
preparing this experiment. Three Pis establish the physical baseline; five
extend it if available. Three/five physical workers do **not** prove the
separate thirty-worker target, and this profile does not retroactively turn
the local inconclusive curve into a pass.

## Hardware and OS

Use the Pis already available; report each model and RAM size before choosing
the build/rate profile. Prefer a homogeneous group for the first paired
overhead experiment. If models differ, record each role and analyse them
separately rather than describing the group as identical hardware.

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
that driver is not implemented by these two commands.

## Software/artifacts for the actual experiment

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
minimal worker). The existing probe still needs the remote-task extension
below; its present successful build does not make it a remote load generator.
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

The planned starting offer is 500 requests/s/worker with two clients and the
existing ten-second window, but this is **not yet hardware-qualified**. First
run a bounded formation/rate pilot. If a Pi cannot sustain ≥99% delivery, retain
that failure and review a lower offered rate **prospectively**, as a differently
named experiment—not a pass at 500 requests/s or a relaxed loss gate.
Keep six modes/six rotated rounds and the below-10% normal CPU/p95 goals unless
the operator reviews a different physical-host plan. Five-node support,
cross-host synchronization/deadlines, elapsed budget and source freeze require
their own explicit profile; no new acceptance batch starts from this guide.

## Current verification and next handoff

Local helper tests cover 3/5-node inventory bounds, duplicates/injection,
strict SSH options/quoting, private create-only preparation, ELF/hash checks,
bounded child IO/deadlines and retained per-host failure/mismatch outcomes.
Verification on 2026-09-10: all 14 helper tests and 36 existing formation
harness tests passed, along with formatting, diff and documentation checks.
Reproduce the helper checks with `python3 scripts/test_pi_lab.py` and the
existing harness checks with `python3 scripts/test_formation_telemetry.py`.
They do not prove SSH connectivity, ARM64 execution, firmware readings,
cross-host formation or performance. The readiness scripts are useful now;
the [implementation task](tasks/implement-physical-formation-experiment.md)
tracks the missing experiment driver without claiming it is delivered.

User inputs when ready: Pi model/RAM per role, chosen OS image, 3 or 5 nodes,
SSH aliases/usernames and lab IP/interface details, coordinator address and a
future experiment time budget. No credentials should be pasted into chat.
