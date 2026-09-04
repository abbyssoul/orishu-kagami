# Documentation

Everything in this directory describes the Orishu Kagami project as a whole.
Binary-specific usage belongs beside that binary under `apps/`; durable
cross-cutting design decisions belong in `docs/adr/`.

Start here:

- [Architecture](architecture.md) — product roles, ownership, and runtime flow.
- [Simulation plugins](simulation-plugins.md) — how advanced users add physical
  models without extending trusted Kagami or worker code.
- [Workload contract](protocol-workload.md) — the portable sandbox boundary and
  lifecycle implemented by client-supplied simulation packages.
- [What is an Orishu workload?](workloads.md) — the user-facing definition,
  contents, lifecycle, identity, and recommended delivery model.
- [Development](development.md) — toolchain, commands, and validation.
- [Releasing](releasing.md) — continuous integration and delivery behaviour.
- [Coding style](Coding%20style.md) — application-neutral Rust design guidance.
- [High-performance Rust](high-performance-rust.md) — measurement and hot-path guidance.
- [Target experiments](target-experiments.md) — long-term scientific benchmarks.
- [Migration strategy](migration.md) — how Orishu, Field CAD, and Kagami source
  material is evaluated and reused.
- [Project context](../CONTEXT.md) — canonical product language and invariants.
- [Architecture decisions](adr/README.md) — decisions that should not be
  rediscovered in code review.
- [Tracked tasks](tasks/README.md) — refined, implementation-ready work promoted
  from the project-ideas inbox.

The source repositories remain historical references during migration. Their
documentation is not automatically authoritative here: adopted behaviour must
be reconciled with the project context and recorded in this repository.
