# Implement worker network placement

Status: **planned; bounded single-interface slice prioritized for PoC support;
multi-interface production behavior requires a design decision**.
Owner: N-FORMATION network IO shell and operator configuration.

## Outcome and current gap

Operators select a bounded list of internal interfaces for peer communication
and an independently configured list for HTTP/client administration. Explicit
placement must cover listening **and outbound/reply traffic**, not merely the
address published to other nodes. This supports bare metal, containers and
Kubernetes without making interface names part of membership identity.

Today, repeated `--listen.clients` selects multiple TCP/Unix addresses;
`--listen.peers` and `--advertise.peers` each select one literal socket address.
The YAML peer lists also reject more than one element. There is no interface
allowlist or device-bound egress policy. Initial admission creates a separate
wildcard-bound outbound QUIC endpoint; peer listening does not constrain it.
See the [current worker manual](../../apps/orishu-worker/README.md#network-placement-current-limitations).

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
