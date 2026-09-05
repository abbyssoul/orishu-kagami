# Tracked tasks

Tasks refine accepted product and architecture decisions into bounded,
verifiable implementation work. `TODO.md` remains an inbox for unrefined ideas;
once promoted, an idea links to a task here.

See the [implementation roadmap](../roadmap/README.md) for milestone order,
cross-task dependencies, unrefined work packages, and parallel-agent ownership.

| Task | Status |
| --- | --- |
| [Migrate and integrate the shared variables subsystem](migrate-and-integrate-variables-subsystem.md) | Ready |
| [Implement the Kagami MCP server](kagami-mcp-server.md) | Ready; later slices gated |
| [Implement the Kagami object catalog](implement-kagami-object-catalog.md) | Core ready; integration gated |
| [Define and adopt the shared workload format](define-and-adopt-shared-workload-format.md) | Ready; foundational priority |
| [Implement resumable observation streaming](implement-resumable-observation-streaming.md) | Ready |
| [Implement time-addressable run playback](implement-time-addressable-run-playback.md) | Ready |
| [Implement the sans-IO cluster membership core](implement-membership-model.md) | Implemented and accepted |
| [Fix membership liveness propagation through full-record merge](fix-membership-liveness-gossip-merge.md) | Implemented and accepted |
| [Implement the operational cluster-formation PoC](implement-cluster-formation-poc.md) | Contract slice ready; membership accepted; adapters follow preflight |
| [Implement worker operational observability](implement-worker-observability.md) | Contract slice ready; formation/runtime instrumentation follows owning services |
| [Document and verify operator observability workflows](document-worker-observability.md) | Planned; ships alongside observability slices |
