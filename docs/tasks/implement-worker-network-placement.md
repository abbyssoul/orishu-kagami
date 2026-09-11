# Implement worker network placement

Status: **slices 1-2 delivered — contract recorded in
[ADR 0026](../adr/0026-worker-network-interface-placement.md) and one selected
interface per role enforced on Linux with verified namespace wire evidence;
physical Pi revalidation and the remaining slices 3-4 are still planned**.
Owner: N-FORMATION network IO shell and operator configuration.

Post-M4 targets: physical single-interface recheck in early M5; slices 3–4
design review in M5 and production implementation/qualification target M8.
Endpoint selection, failover and migration require the existing decision gate;
they are not selected by this schedule. Track any explicit deferral in the
[post-M4 register](../roadmap/README.md#post-m4-operational-follow-ups).
[P-DEPLOY](qualify-worker-deployment-profiles.md) owns the environment/support
matrix and reuses this task's wire proof, rather than duplicating socket work.

## Outcome and current gap

Operators select a bounded list of internal interfaces for peer communication
and an independently configured list for HTTP/client administration. Explicit
placement must cover listening **and outbound/reply traffic**, not merely the
address published to other nodes. This supports bare metal, containers and
Kubernetes without making interface names part of membership identity.

Repeated `--listen.clients` selects multiple TCP/Unix addresses; `--listen.peers`
and `--advertise.peers` each select one literal socket address, and the YAML peer
lists still reject more than one element. `--interface.peers` and
`--interface.clients` now bind every socket of their role to one named device,
including the outbound QUIC endpoint used for initial admission, which was
previously wildcard-bound and unconstrained by peer listening. There is still no
interface *allowlist*: one interface per role, Linux only, no failover.
See the [worker manual](../../apps/orishu-worker/README.md#network-placement).

The [Pi routing experiment](../measurements/formation-pi-ethernet-routing-2026-09-11.md)
demonstrated the practical gap: three hosts selected Wi-Fi for replies even
when servers bound their Ethernet IPs. Temporary OS routing corrected the
experiment, but is not a worker-enforced placement contract. Prioritize the
smallest enforceable slice before further network-isolation qualification;
do not require full multi-homing to retain the current diagnostic results.
This does not reopen historical N-FORMATION acceptance or close M4 overhead.

## Bounded slices

1. **Resolve and record the contract.** Write an ADR and link it from the ADR
   index before implementing socket/configuration semantics. Compare explicit
   addresses with OS-managed routes, per-socket device/source constraints, and
   deployment-owned network namespaces. Explain which combinations can enforce
   ingress and egress, their privileges and unsupported-platform behavior.
   Decide address resolution, IPv4/IPv6 scope, wildcard interaction, duplicate
   selection, advertised endpoints, startup validation and interface loss.
   Define bounded configuration fields and precedence; do not publish invented
   CLI flags as implemented. Startup-only configuration is sufficient for PoC.
2. **PoC support: one selected interface per network role.** Implement the
   reviewed minimum on the Linux lab profile, allowing peers and TCP clients to
   select different interfaces (or deliberately share one). Audit every socket
   creation path: initial join/retry, admitted dialing/reconnection, catch-up,
   SWIM/gossip and TCP accept/replies. Explicit constraints must fail clearly
   when unavailable or unenforceable, never silently fall back to another
   interface. Do not have workers rewrite host routing tables or firewall rules.
   Preserve the existing unconfigured behavior and local Unix administration.
3. **Bounded lists and production portability.** Extend to multiple selected
   interfaces/addresses per role after deciding peer endpoint selection,
   failover, address changes and QUIC migration. Document what interface means
   inside a container/pod namespace, capability requirements and supported
   rootless/Kubernetes/cloud deployments. Host-interface names must not be
   assumed visible in a pod. Version any changed wire/resource contract.
4. **Operator handoff.** Update worker CLI/YAML/environment examples, deployment
   recipes, the Pi guide and the linked worker/cluster admin stories. Provide
   bounded, secret-free inspection of requested and resolved placement; carry
   appropriate read-only inspection into `orishuctl` and the monitor's API
   integration task. Include wrong-route/interface-down troubleshooting and
   rollback. Do not expose credentials or add unbounded interface metric labels.

## Delivered increment: one interface per role — 2026-09-11

Slice 1 is recorded as [ADR 0026](../adr/0026-worker-network-interface-placement.md).
It compares address selection, OS routes, per-socket device binding and
deployment-owned namespaces, and selects `SO_BINDTODEVICE` for the PoC: it
constrains ingress and egress, mutates no host state, and needs no added
capability on current Linux kernels. `socket2` was already resolved in the tree
through quinn-udp and tokio, so naming it adds no new code.

Slice 2 landed in `apps/orishu-worker`. `--interface.peers` /
`--interface.clients` (plus `ORISHU_INTERFACE_*` and `spec.interface.*`) bind
every socket of their role in `net_placement.rs`. All four production socket
paths were audited: the QUIC peer listener and the lazily created outbound join
endpoint both go through `placed_udp_socket`, and TCP client listeners go
through `placed_tcp_listener` behind a small salvo `Listener` adapter that keeps
the existing TLS composition. Catch-up, SWIM/gossip and reconnection create no
sockets of their own — they reuse registered connections on the placed endpoint.
Unix client sockets and the loopback diagnostics listener are deliberately
unplaced, so ADR 0017's exposure policy is unchanged.

Two semantics were settled against the kernel rather than assumed. A socket
bound to a device with no usable route fails its route lookup instead of
selecting another device, which is what makes this enforcement rather than
preference. A concrete bind address paired with a *different* device binds
successfully and then matches no ingress, and neither a bind probe nor a
device-scoped route lookup distinguishes that from a correct pairing — so
placing a role requires a wildcard bind, and the combination is refused.

The [two-namespace veth wire proof](../measurements/worker-network-placement-2026-09-11.md)
(`make test-worker-network-placement`, or the isolated user-namespace recipe in
the report) covers seven scenarios: peer misrouting control, exclusion, placed
formation and interface loss/recovery, client reachability control and placed
client enforcement, and simultaneous peers/clients on different devices.
Each role has its own negative control, because client placement
binds different sockets and replies over accepted connections rather than
inheriting the peer verdict. The client checks open fresh connections and issue
authenticated `GET /api/v1/cluster` requests, so they exercise real application
replies rather than a bare handshake. Missing/wrong credentials must yield 401;
wrong IP SAN and untrusted certificates must fail TLS. Those failures are
distinguished from network exclusion, so a broken harness cannot read as a pass.

A peer run counts only when catch-up has completed, both sides are introducer
ready, and their exact two live member IDs and certificate fingerprints match
the initial identities and the assignment in the completed join operation.
Matching formation IDs alone, or a stalled `catchingUp`, is
reported as such rather than passing or masquerading as correct exclusion. The
interface-loss scenario requires a working formation before disruption, creates
a fresh replicated lock while disconnected, proves it has not reached the other
worker, then restores the link and route and requires that lock to cross with
the same verified membership. Restoration runs even on failure. Later scenarios
do not inherit a broken topology. Carried-traffic thresholds are per role and
measured: the client probe moved 20,981 bytes against 0 on an idle window, so
one peer-sized threshold would have misread a real client check as no traffic.
The harness never reclaims namespaces or veth devices implicitly — it refuses to
start when its names are already in use and offers `--cleanup` for leftovers, so
it cannot destroy a concurrent run. Shutdown must be clean and bounded, with no
forced worker kill or leftover Unix socket. `--output` retains a private report
and sibling evidence directory, including failure evidence, without overwriting
an earlier report. The evidence directory contains disposable credentials and
must not be committed or published.

The [verified run](../measurements/worker-network-placement-2026-09-11.md) closes
the local Linux namespace wire-proof gap, not physical deployment qualification.
Physical Pi re-verification, bounded multi-interface lists, failover, container/pod
namespace semantics and `orishuctl`/monitor inspection all remain open.

## Acceptance criteria

- Configuration tests reject excessive lists, duplicates, missing interfaces,
  ambiguous address selection, unsupported constraints and unsafe wildcard or
  advertisement combinations before exposing partially configured listeners.
- Real socket tests in a controlled namespace/veth topology deliberately prefer
  the wrong route. Packet/interface evidence proves initial admission,
  steady-state peer traffic and client replies obey the selected role policy.
  Interface loss cannot silently move traffic onto an excluded interface.
- Formation, catch-up, reconnect and bounded shutdown pass with the constrained
  paths. All sockets represent the same process/node/certificate identity;
  connection, queue and dial budgets remain global rather than multiplying per
  listener. Network selection never grants admission or bypasses authentication.
- TLS certificate names/IP SANs cover actual client endpoints. Peer mTLS and
  operator authorization remain independent. ADR 0017's loopback diagnostics
  and authenticated monitoring proxy are not broadened by client placement.
- Verify the minimum on the Pis, retaining source-route checks, per-interface
  counter evidence, interface identity and cleanup. Route evidence must cover
  the measurement window, including any timed rollback. Do not infer isolation
  from inventory addresses or link-up status alone.
- Document limitations for bare metal, rootless containers and pod namespaces;
  unsupported enforcement produces a clear error, not an isolation claim.
  Run relevant configuration, real-wire/lifecycle and documentation checks.

## Non-goals

No new node identity, membership authority, automatic host-network management,
load balancer, general service discovery, transparent NAT traversal, runtime
hot reload, or numerical workload performance claim. Full multi-interface
failover is not required for the initial single-interface PoC slice.

## Related work

- [Worker operator story](../user-stories/orishu/worker-admin.md#separate-peer-and-client-network-interfaces)
- [Physical experiment](implement-physical-formation-experiment.md)
- [Formation PoC](implement-cluster-formation-poc.md)
- [Monitor API integration](integrate-orishu-monitor-operator-api.md)
- [ADR 0013: identity](../adr/0013-cluster-formation-and-node-identity.md)
- [ADR 0017: observability](../adr/0017-worker-operational-observability.md)
