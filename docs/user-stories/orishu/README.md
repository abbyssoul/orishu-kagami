# User stories: orishu

`orishu` is a decentralized runtime for scientific simulation where a cluster of worker nodes cooperatively advances one shared, deterministic workload instead of running many unrelated jobs under a central scheduler.

The product keeps this intentionally simple: one cluster runs one workload at a time. It is built for two audiences: cluster users who want to run scientific computations and study results, and administrators who make those workloads possible by setting up, operating, and safeguarding the cluster.

Stories in this directory are written from the perspective of a cluster user or administrator interacting with `orishu` directly (over its protocol or CLI), not from the perspective of a Kagami author. Kagami-facing stories live in [docs/user-stories/kagami](../kagami/README.md).

## Personas

### Cluster user

- **Moniker:** cluster user
- **Goals and motivations:** Run meaningful scientific computations, observe progress, inspect intermediate state, and study results without having to think like a distributed-systems engineer. Get trustworthy, reproducible outcomes from a shared cluster.
- **Pain points and frustrations:** Existing systems often feel like generic schedulers, expose too much infrastructure complexity, make run state hard to understand, and provide weak guarantees around determinism, compatibility, or resumability.
- **Key tasks/usage scenarios:** Check workload compatibility, load a workload, start or step a simulation, stop or resume from checkpoints, monitor workload status, and retrieve results for analysis.

### Administrator

- **Moniker:** administrator
- **Goals and motivations:** Make scientific computation possible by deploying workers, forming healthy clusters, keeping them safe and understandable, and giving cluster users a reliable platform to run workloads.
- **Pain points and frustrations:** Existing distributed systems can be overly complex to bootstrap, opaque when something goes wrong, risky to change during live operation, and burdensome to secure in adversarial network conditions.
- **Key tasks/usage scenarios:** Start and configure `orishu-worker`, inspect nodes and cluster status, control cluster admission, configure node participation at startup, add or remove nodes, lock or unlock membership, diagnose health and connectivity, and maintain safe day-2 operations.

Workload components containing plugin model implementations are authored
outside the cluster using external tooling. Numerical kernels are internal
algorithms/libraries used to build those components; see the
[Kagami personas](../kagami/README.md#personas).
Cluster users and administrators consume them as opaque, content-addressed
artifacts that workers validate and execute as hostile code (see
[ADR 0009](../../adr/0009-execute-workloads-as-sandboxed-portable-programs.md)).

## Glossary

- **Cluster user:** The researcher or scientific practitioner who uses `orishu` to run a workload, monitor progress, manage simulation state, and study results.
- **Administrator:** The person who installs workers, configures nodes, forms and maintains clusters, and keeps the platform safe and usable for cluster users.
- **Node:** A cluster member as seen from cluster operations and runtime behavior. In practice, a node is represented by a running worker and its cluster-assigned identity.
- **Worker:** A running `orishu-worker` process that participates in a cluster by storing state, communicating with peers, and executing its share of the simulation.
- **Standalone node:** A worker that is not currently a member of a multi-node cluster. It behaves as a cluster of one until it joins another cluster.
- **Introducer:** A node that listens for peer join requests from other workers.
- **Workload:** The immutable simulation definition currently active in a
  cluster: manifest, component graph, initial conditions, inputs and
  requirements. Epoch, placement and run state are separate runtime state.
- **Workload component:** A content-addressed WebAssembly Component containing
  executable plugin model code. A workload may configure several component
  instances in a host-orchestrated graph. Workers execute assigned instances as
  untrusted guests under the versioned lifecycle contract (see
  [ADR 0024](../../adr/0024-orishu-orchestrates-a-workload-component-graph.md)).
- **Numerical kernel:** An algorithm or library used inside a workload
  component; not a directly loaded plugin or runtime authority.
- **Run:** One execution of an immutable workload in a particular workload
  epoch, producing ordered observations and result/checkpoint artifacts.
- **Run reference:** Shareable identification of a run by cluster formation,
  workload identity, and workload epoch, with optional connection hints but no
  credentials, control capability, or presentation state.
- **Observation:** Provenance-bearing output at a committed simulation boundary.
  Several clients may independently read live or persisted observations from
  the same run.
- **Admission:** The process by which a worker is allowed to join a cluster, including identity checks, policy checks, and any token or mTLS requirements.
- **Re-admission:** Allowing a previously removed or blocklisted node to go through admission again.
- **Cluster membership:** The current set of nodes that belong to a cluster.
- **Membership lock:** A cluster-scoped administrative state that prevents administrator-initiated membership changes until unlocked.

## Permissions model

These personas describe user goals, not hard security roles. In practice, access is enforced by auth tiers and locality rules: some read-only actions are allowed locally with lower trust, while cluster-changing or destructive actions require higher privileges.

## Reading order

This directory captures user-facing behavior from three angles: setting up and configuring individual workers, operating a cluster, and managing workloads once the cluster is running.

1. [worker-admin.md](./worker-admin.md) - start here for single-node setup and worker configuration.
2. [cluster-admin.md](./cluster-admin.md) - then read cluster operations, health, topology, and admin controls.
3. [workload.md](./workload.md) - finish with workload lifecycle, simulation control, and result-oriented workflows.
