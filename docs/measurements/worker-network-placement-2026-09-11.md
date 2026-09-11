# Worker network placement — Linux namespace verification, 2026-09-11

Result: the seven-case wire proof passed on the source-built Linux profile.
This closes the namespace-verification gaps in the
[placement task](../tasks/implement-worker-network-placement.md), not physical Pi
qualification, production multi-homing or the M4 performance/overhead gate.
No Pi binaries, inventories, host interfaces or host routes were changed.

## Scope and method

Two disposable network namespaces each contain a real worker, joined by two
veth pairs on the same IPv4 subnet. The excluded route has metric 100 and the
selected route metric 600. Exact-source route lookups verify that the kernel
really prefers the excluded path before testing enforcement. Both workers
advertise their own selected address, not a shared endpoint.

A successful formation requires the completed join operation, the pinned target
formation, the original introducer ID, the assigned applicant ID, both original
certificate fingerprints, exactly two live members on each side, and both
introducers ready. Counts or formation-ID adoption alone cannot pass. Admission
followed by stalled catch-up cannot count as successful interface exclusion.

Interface-loss verification first requires that complete working formation. It
drops the selected link, makes a fresh authenticated lock change on one worker,
and requires that change to remain absent on the other. Link and route
restoration must then carry the change to the other worker with the same pinned
membership. This proves fresh post-restoration propagation; it does not claim
that a new QUIC handshake was forced.

Client checks make four fresh TLS connections and authenticated HTTP requests,
validating the IP SAN and certificate trust. Missing/wrong tokens must return
401. Wrong-IP and untrusted certificates must fail TLS, not be classified as
network exclusion. A separate case forms peers on one device while serving
client requests on the other and rejecting client requests through the peer
device.

## Recorded results

The retained seven-case run completed in **83.27 seconds**, including the
intentional exclusion deadline. Bytes below are summed transmit-counter deltas
across the two endpoints, not application throughput. Small ARP/background
deltas are not treated as carried peer traffic (8,192-byte threshold; the
separate client threshold is 2,048 bytes).

| Scenario | Selected TX bytes | Excluded TX bytes | Observed outcome |
| --- | ---: | ---: | --- |
| Unplaced peer control | 180 | 40,222 | Exact formation succeeds over the wrong path |
| Only introducer placed | 900 | 12,066 | Applicant retries, no admission/catch-up accepted |
| Both peers placed | 40,028 | 0 | Exact formation succeeds over selected path |
| Selected link down | 0 | 70 | Fresh lock remains local; no traffic relocation |
| Link restored, same loss case | 15,067 | 0 | Fresh lock propagates; exact membership still valid |
| Unplaced client control, excluded request | 0 | 21,293 | Four authenticated responses |
| Placed client, selected request | 21,297 | 0 | Four authenticated responses; excluded requests fail |
| Split roles, peer formation window | 40,156 | 0 | Peers form on selected device; four client requests then succeed on the other device |

Both client security negative controls returned 401; wrong IP and untrusted
certificate checks were rejected. All **12 worker processes exited 0**, none
needed a forced kill, every local Unix socket was removed, and namespace/device
cleanup verification passed. The additional restored-link row is part of the
loss scenario, not an eighth independent scenario.

## Reproduction and retained evidence

Host: Linux `6.17.0-41-generic`, x86_64; Rust `1.97.1`; Python `3.13.7`;
iproute2 `6.16.0`; OpenSSL `3.5.3`. Source base is
`370b0ead7449be7096dd0096933632f1e31ff794` plus the uncommitted placement work.
This is a correctness check using debug binaries, not a performance benchmark.

Prerequisites: built worker and control binaries, Python, iproute2, OpenSSL and
util-linux. On a Linux host permitting unprivileged user namespaces, run from
the repository root:

```sh
cargo build --locked -p orishu-worker -p orishu-ctl
python3 -m unittest discover -s scripts -p 'test_worker_network_placement*.py'
unshare -Urnm --fork sh -c 'mount --make-rprivate / && mount -t tmpfs tmpfs /run && python3 scripts/check-worker-network-placement.py --output /tmp/orishu-placement-new-run.json'
```

Use a fresh output name each time. `-U -r` maps the caller to root only inside a
new user namespace; `-n -m` isolate networking and mounts. Making mounts private
and mounting a temporary `/run` keeps named network-namespace mounts out of the
host's `/run`. The worker runs within this isolated lab. This host allowed the
recipe without sudo; it does **not** establish support for arbitrary rootless
container networking or Kubernetes plugins. If user namespaces are disallowed,
the existing explicit `make test-worker-network-placement` sudo target is the
alternative; do not weaken host policy automatically.

The harness refuses existing topology names and removes only resources created
by its own run. Its explicit `--cleanup` mode is for operator-confirmed stale
resources, never normal startup or concurrent-run recovery.

Local report: `/tmp/orishu-placement-verified-wire.json`, SHA-256
`29db8fab3af8046607c5bace6a27f39dd0815b17343bb4b198fd29540401c1c3`.
The sibling `/tmp/orishu-placement-verified-wire.evidence` retains private logs,
state and disposable credentials. Reports are mode 0600 and evidence directories
0700; **do not commit or publish the evidence directory**. They are local `/tmp`
artifacts, not permanent repository storage. Failed runs also retain evidence
when `--output` is supplied. An earlier run exposed a report-writer API error;
that run is not counted as a pass and the writer now has a regression test.

Binary SHA-256:

- `target/debug/orishu-worker`: `f6d007a3a1fcc3c3c228f69be7de133e48e7a8a102e716b29f9a5c74cd06c078`
- `target/debug/orishuctl`: `3bd7cf4d8029c187f989f5a585becb1790aa6df90a66de736256bc4e854720f0`

The final repeat, including an explicit selected-path assertion for the
split-role formation, also passed in **82.64 seconds**, again with all 12 workers
shut down cleanly and no topology leftovers. Report:
`/tmp/orishu-placement-final-pass.json`, SHA-256
`da0a7d818ac07f35cabd1c1e4247da5372293a1d86ee3e326762adefbd50801f`;
private evidence: `/tmp/orishu-placement-final-pass.evidence` (same restrictions).
The binaries were unchanged. Final harness SHA-256:

- `scripts/check-worker-network-placement.py`: `f80530bd06d5b8dfe99388044a24bf31a0274b08e365c412a4abf30f504a9a8e`
- `scripts/worker_placement_topology.py`: `bcf08f65b83ef8be5c5a1c3fd95980d7fd08f61f0e18f311f581a70281ebc8e7`

## Regression checks and remaining work

The repaired verification uses the real coordinator and real TLS probe rather
than only testing look-alike verdict logic. **51 Python tests** cover identity,
readiness, route/counter verdicts, lifecycle restoration, namespace ownership,
private evidence persistence and authenticated TLS behavior. **22 focused Rust
tests** cover configuration, process startup and socket placement. Worker
all-target Clippy and workspace formatting checks pass. Non-Linux expectations
are platform-conditional, but no non-Linux execution was performed here.

Next physical follow-up: rebuild and deploy this worker to the Pi fleet, select
the actual Ethernet interface independently for each role, replace the temporary
route correction with worker flags, and retain source-route/counter evidence and
cleanup. Multi-interface lists, failover, deployment-specific qualification and
remote placement inspection remain the planned later slices. No new approval
decision is needed to use the completed local verification evidence.
