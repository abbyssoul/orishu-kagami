# Qualify selected worker deployment profiles

Status: **specified; environment selection and execution remain gated**.
Owner: **P-DEPLOY**, coordinating N-NETWORK-PLACEMENT, P-INSTALL and P-OBS-DOCS.
Target: select candidate profiles during M5; qualify every claimed supported
profile for M8. Kubernetes and cloud-provider profiles are optional candidates,
not promised release support.

## Outcome and current gap

Operators can distinguish a tested deployment recipe from a supported platform
claim. The existing source-built systemd/rootless-container recipes and Linux
namespace placement proof cover narrow boundaries; they do not qualify arbitrary
container networks, Kubernetes plugins or cloud infrastructure.

## Decisions before implementation or provisioning

Record an explicit matrix of selected, experimental, deferred and unsupported
profiles: OS/architecture, supervisor/runtime, network namespace model,
capabilities, address families, ingress/egress policy, monitoring and credential
provisioning. For optional Kubernetes select the distribution/network plugin;
for optional cloud profiles select provider, network shape and cost/time limits.
Do not assume host device names exist inside pods. Selection is not permission
to create billable resources or alter an existing cluster/network.

## Bounded slices

1. Map existing evidence to the matrix and identify the smallest missing
   assertion for each selected profile. Preserve the separate source-build and
   [published-artifact](publish-installable-artifacts.md) support boundaries.
2. Supply opt-in, bounded setup/check/cleanup recipes for one selected profile
   at a time. Record resource ownership, prerequisites, private credentials,
   watchdog/rollback behavior and unsupported enforcement errors. Use the
   [network-placement contract](../adr/0026-worker-network-interface-placement.md);
   do not invent failover or worker-owned routing policy.
3. Verify real peer formation/catch-up and recovery, independent client access,
   advertised reachability, ingress and reply paths, TLS/operator authority,
   selected-link loss and restoration, and bounded shutdown. Include excluded
   paths and failed privilege/interface selection, not only successful joins.
4. Verify the profile's scrape/probe/Collector and log collection recipe with
   outages and recovery. Keep diagnostics exposure unchanged. An optional
   Kubernetes example consumes these contracts; it does not add a scheduler or
   Kubernetes operator to Orishu.
5. For M8, retest selected supported profiles with the exact release artifacts.
   Update installation support tables, worker/cluster operator stories, manuals,
   deployment examples and incident/rollback guidance. Give each unqualified
   candidate an explicit deferred/unsupported disposition and review point.

## Acceptance criteria

- Every support claim maps to a named profile, exact artifact/configuration,
  real-wire evidence and reproducible clean installation/lifecycle recipe.
- Namespace, capability, TLS and network-policy failures cannot silently broaden
  exposure. Cleanup removes only owned resources; reports expose no secrets.
- Optional Kubernetes/cloud profiles may remain deferred at M8, but must not
  appear as supported in release documentation. The matrix itself is reviewed,
  rather than requiring qualification of every possible platform.
- Publish evidence links and update the
  [follow-up register](../roadmap/README.md#post-m4-operational-follow-ups).

## Non-goals

General cloud certification, managed hosting, universal CNI support, mandatory
Kubernetes, automatic provisioning in user accounts, or reopening M4 acceptance.
