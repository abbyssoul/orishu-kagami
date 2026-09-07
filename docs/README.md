# Documentation

Everything in this directory describes the Orishu Kagami project as a whole.
Binary-specific usage belongs beside that binary under `apps/`; durable
cross-cutting design decisions belong in `docs/adr/`.

Start here:

- [Why Orishu exists](why-orishu-exists.md) — product motivation, audiences,
  objectives, success measures, and explicit non-goals.
- [Architecture](architecture.md) — product roles, ownership, and runtime flow.
- [Orishu runtime design](orishu-runtime-design.md) — decentralized worker
  semantics assumed by the client and peer protocols.
- [Orishu configuration](orishu-configuration.md) — shared configuration,
  precedence, startup, and trust rules.
- [Orishu operational observability](orishu-observability.md) — implemented local
  metrics/probes and local OTLP traces, with remaining operator delivery gates.
- [Interrupted admission recovery](cluster-admission-recovery.md) — bounded
  operator inspection, existing-attempt recovery and mandatory stop conditions.
- [Implementation roadmap](roadmap/README.md) — milestone gates, workstreams,
  dependencies, parallel lanes, and exit criteria.
- [Simulation plugins](simulation-plugins.md) — how advanced users add physical
  models without extending trusted Kagami or worker code.
- [Workload contract](protocol-workload.md) — the portable sandbox boundary and
  lifecycle implemented by client-supplied simulation packages.
- [What is an Orishu workload?](workloads.md) — the user-facing definition,
  contents, lifecycle, identity, and recommended delivery model.
- [The shared resource envelope](resource-envelope.md) — the one structural
  shape Orishu resources and Kagami object templates share, and the authority,
  identity, and validation each keeps.
- [Installation](install.md) — source workflows available today, packaging
  scaffolds, planned release channels, and their support status.
- [Development](development.md) — toolchain, commands, and validation.
- [Worker Prometheus acceptance](testing-worker-prometheus.md) — pinned parser,
  local scrape example and real source-build ingestion checks.
- [Worker Collector walkthrough](testing-worker-otelcol.md) — pinned local
  OTLP receiver, inspectable spans and real collector outage/recovery checks.
- [Releasing](releasing.md) — continuous integration and delivery behaviour.
- [Coding style](Coding%20style.md) — application-neutral Rust design guidance.
- [High-performance Rust](high-performance-rust.md) — measurement and hot-path guidance.
- [Target experiments](target-experiments.md) — long-term scientific benchmarks.
- [Orishu scaling objectives](orishu-scaling-objectives.md) — runtime-platform
  scale, capacity, resilience, and evidence targets.
- [Orishu runtime data model](orishu-data-model.md) — authority, durability,
  mutability, and relationships among runtime entities.
- [Artifact storage specification](storage-spec.md) — normative storage
  authority, safety, repair, deletion, and failure semantics.
- [Orishu output provenance](orishu-provenance.md) — authoritative artifact
  lineage versus diagnostic execution evidence.
- [Orishu deferred design work](orishu-runtime-future-work.md) — proposals and
  open questions that are not current behavior.
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
