# 0026 — Bind each network role's sockets to one explicitly named interface

Status: **accepted — one interface per role; PoC enforcement is Linux-only**
Decision date: **2026-09-11**

## Context

The worker selects peer and client *addresses* and nothing else. `--listen.clients`
takes repeated TCP/Unix addresses; `--listen.peers` and `--advertise.peers` each
take one literal socket address. Nothing names an interface, and initial admission
creates a separate wildcard-bound outbound QUIC endpoint that no peer setting
constrains.

Address selection does not place traffic. The
[Pi routing experiment](../measurements/formation-pi-ethernet-routing-2026-09-11.md)
ran sixteen workers on four hosts that each carried Ethernet and Wi-Fi addresses
on one subnet. Three of four hosts preferred the lower-metric wireless route, so
replies left over Wi-Fi although every server socket was bound to its Ethernet
address. A temporary operator-installed route corrected the experiment and raised
the peak from 63,361 to 94,280 requests per second, but an `ip route` command
typed over SSH behind a deletion timer is not a worker-enforced contract. The
measurement's own conclusion is the premise of this record: address binding alone
is not the capability.

Operators need peer traffic and client administration to follow deployment
network policy on bare metal, in containers and under Kubernetes, without making
interface names part of membership identity ([ADR 0013](0013-cluster-formation-and-node-identity.md))
and without broadening the loopback diagnostics exposure of
[ADR 0017](0017-worker-operational-observability.md).

## Decision

### Per-socket device binding

Each network role — peers and TCP clients — may select one interface by name.
The worker binds every socket it creates for a placed role to that role's device
with `SO_BINDTODEVICE` before the socket carries traffic. This constrains ingress
and egress, is per-process, mutates no host state, and requires no added
capability on current Linux kernels. The worker never writes host routing tables
or firewall rules; arranging the surrounding network remains the deployment's job.

Device binding is enforcement, not configuration discovery. An interface name
never supplies an address: bind addresses still come from `--listen.peers` and
`--listen.clients`, and advertised endpoints still come from `--advertise.peers`.
Placement never derives, rewrites or infers an advertised address.

The two role selections are independent. Naming the same interface for both roles
is permitted and is an explicit operator choice, not a fallback. Unix client
sockets have no interface and are never placed. The optional diagnostics listener
keeps its own loopback-only exposure policy and is not placed; client placement
does not broaden it.

### Configuration and precedence

Placement is expressed as `--interface.peers` and `--interface.clients`, the
environment variables `ORISHU_INTERFACE_PEERS` and `ORISHU_INTERFACE_CLIENTS`,
and the YAML fields `spec.interface.peers` and `spec.interface.clients`.
Precedence is file, then environment, then command line, matching the existing
listen and advertise settings. Configuration is startup-only; there is no runtime
hot reload, and an operator changes placement by restarting the worker.

An interface name is a bounded domain value, not a free string: one to fifteen
bytes, beginning with an ASCII alphanumeric, continuing with ASCII alphanumerics,
underscore, dot or hyphen. This is the kernel's `IFNAMSIZ` limit and the same
grammar the Pi lab inventory already validates, so the two surfaces cannot
disagree about what an interface name is.

### Resolution, scope and wildcard interaction

A named interface requires a wildcard bind address; the device performs the
constraining, and `--advertise.peers` is still required to publish a reachable
peer endpoint, exactly as it is today. A concrete bind address combined with a
named interface is rejected. The two can disagree silently: the kernel accepts a
socket bound to a local address belonging to a *different* device, and that
socket then matches no ingress at all while reporting a successful bind. Only
addresses local to no interface are refused by the kernel, and neither a bind
probe nor a device-scoped route lookup distinguishes the wrong-device case.
Refusing the combination removes that failure mode outright rather than
half-checking it, and the operator writes `0.0.0.0:6655` or `[::]:6655` and names
the interface instead. Device binding is address-family independent, so IPv4 and
IPv6 placement use one mechanism.

Every configured name must resolve to an existing device at startup. Validation
completes before any listener is exposed, so a rejected configuration never
leaves a partially bound worker reachable.

### Failure, loss and platform support

An unavailable or unenforceable placement fails startup with an error naming the
interface. The worker never falls back to another interface, never downgrades to
address-only binding, and never reports isolation it cannot enforce.

Losing a placed interface stops that role's traffic; it does not move it onto an
excluded interface. A socket bound to a device with no usable route fails its
route lookup rather than selecting another device, which is the property that
makes this enforcement rather than preference. Recovery is the interface
returning or an operator restart. Automatic failover between interfaces is not
part of this decision.

Enforcement is Linux-only in this decision. On any other platform, a configured
placement is a startup error naming the unsupported platform. Unconfigured
behaviour is unchanged everywhere, including local Unix administration.

Network placement never grants admission, selects or influences node identity,
or replaces authentication. Peer mTLS, client TLS and operator authorization
remain independent, and connection, queue and dial budgets remain global rather
than multiplying per placed listener. Placement introduces no wire, resource or
persisted contract and therefore no version change.

## Alternatives

- Explicit bind addresses alone: already implemented and already shown
  insufficient. They constrain ingress; the route table still chooses egress.
- Accept a concrete bind address alongside a named interface and validate the
  pairing: rejected for this slice. Neither `socket2` nor the worker's existing
  `rustix` dependency can enumerate an interface's addresses, so validation would
  need `getifaddrs` through a new dependency and `unsafe`, to police a
  combination the wildcard form already expresses without ambiguity.
- Per-socket source-address selection without a device: same failure. The Pi run
  bound the intended source addresses and still egressed the wrong interface.
- Worker-managed routes or policy rules: rejected. It would make the worker a
  privileged mutator of shared host state that outlives the process, with no
  safe rollback story on crash.
- Deployment-owned network namespaces only: retained as the recommended
  production answer and the only meaningful isolation inside a pod, but it is
  not something the worker can enforce, validate or report, and it leaves the
  operator with no error when the placement is wrong.
- Bounded multi-interface lists with failover: deferred. Peer endpoint
  selection, address change and QUIC migration need their own decision, and the
  physical-cluster work needs one enforceable interface per role first.

## Consequences and acceptance

Operators gain an enforceable placement contract and lose the ability to be
accidentally isolated-in-name-only. An invalid selection fails startup; a valid
selected device without a working path may start but cannot reach peers, rather
than sending traffic over an excluded interface. That is the intended trade.

Enforcement is bounded in ways operators must be told plainly. Interface names
inside a container or pod namespace refer to that namespace's devices, not the
host's, so host interface names must not be assumed visible. Platforms other
than Linux get a clear unsupported error; Linux reports kernel device-binding
failures explicitly. Rootless/container support depends on the actual namespace
and kernel policy and requires deployment-specific verification, not a blanket
support or rejection claim. Because the worker configuration format accepts and discards unknown
`spec` keys, a misspelled outer YAML key leaves the role unplaced with no error.
The placement block itself rejects unknown keys, and a worker with any placed
role reports its requested and resolved placement at startup, so the *absence*
of that report is the operator's signal that the setting never arrived. That is
a weaker guarantee than a rejection, and the manual states it rather than
leaving it implied.

Before claiming enforcement, verify in a controlled namespace topology that
deliberately prefers the wrong route: a negative control must demonstrate an
unplaced worker misrouting, and the placed run must show per-interface counter
and route evidence that initial admission, steady-state peer traffic and client
replies all follow the selected device, with interface loss stopping traffic
rather than moving it. Formation, catch-up, reconnect and bounded shutdown must
pass over the constrained paths. Configuration rejection must be tested for
unknown interfaces, ambiguous address selection, unsupported platforms and unsafe
wildcard or advertisement combinations. Physical verification on the Pi lab
retains source-route checks, per-interface counters and cleanup evidence;
inventory addresses and link-up status alone do not establish isolation.

## Delivery and references

The [2026-09-11 namespace verification](../measurements/worker-network-placement-2026-09-11.md)
records the bounded Linux wire evidence; physical Pi revalidation and production
deployment qualification remain separate follow-ups.

Delivery: [network placement task](../tasks/implement-worker-network-placement.md),
[worker manual](../../apps/orishu-worker/README.md#network-placement),
[worker operator story](../user-stories/orishu/worker-admin.md#separate-peer-and-client-network-interfaces),
[Pi cluster guide](../testing-worker-pi-cluster.md).
