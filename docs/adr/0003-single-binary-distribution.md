# 0003 — Distribute orishu as a single self-sufficient binary

Status: **accepted**  
Date: **2026-09-04**

## Context

`orishu-worker` does three things: forms and maintains cluster membership,
serves simulation results (checkpoint and result artifacts), and runs live
simulation steps. Splitting these into separate processes would follow the
Unix philosophy of one tool doing one thing, but it does not fit how the
three activities relate to each other or how the product must be deployed and
operated.

All three activities depend on the same prerequisite: an established cluster
membership and composition. Cluster formation and maintenance is itself a
networking activity that produces that membership. Serving simulation
artifacts is a distributed storage activity that must know which nodes hold
which chunks, which depends on membership. Running a simulation requires
whole-cluster coordination for step planning and halo exchanges, which
depends on membership being current. None of the three can act correctly
against a membership view a separate, independently-deployed process might
not share.

Splitting these into separate binaries would mean either duplicating cluster
membership and networking in each process, or introducing an inter-process
protocol and coordination layer purely to share a membership view that a
single process already has for free. That cost buys process-level isolation
`orishu` does not need at this stage, in exchange for real cost to
distribution, scheduling, update, and maintenance: operators would need to
deploy, version, and keep multiple cooperating processes per node in sync
instead of one.

Every other binary in the monorepo (Kagami, CLI tools, future clients) faces a
different constraint: it must be installable with a single `cargo install
<binary>`, which places only the binary on the system, no config files,
scripts, or extra directories.

## Decision

`orishu-worker` is distributed as one binary that forms and maintains cluster
membership, serves simulation results, and runs live simulation, because all
three require the same established cluster membership and composition to
operate correctly.

Every other `orishu`/`kagami` binary must be self-sufficient and runnable
standalone: no companion config files, no generated directories, no setup
step beyond `cargo install <binary>`. A binary that needs persistent local
state creates and manages it itself (e.g. under a standard OS config/cache
directory) rather than depending on anything installed alongside it.

## Consequences

- `orishu-worker`'s internal modules (membership, storage, execution) must
  stay cleanly separated in code even though they ship and run as one
  process; the single binary is a distribution decision, not permission to
  couple those modules internally.
- Operators deploy, schedule, and update one artifact per node instead of
  coordinating versions across cooperating processes.
- New binaries are reviewed against the standalone bar: if a binary would
  require a config file, sidecar script, or pre-created directory to run,
  that requirement must be justified or removed before it ships.
- Process-level isolation between cluster networking, storage serving, and
  simulation execution is not available within a single `orishu-worker`
  process; a future decision to split them would need to solve the shared
  membership dependency this record identifies, not just move code.
