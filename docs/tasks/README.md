# Tracked tasks

Tasks refine accepted product and architecture decisions into bounded,
verifiable implementation work. `TODO.md` remains an inbox for unrefined ideas;
once promoted, an idea links to a task here.

See the [implementation roadmap](../roadmap/README.md) for milestone order,
cross-task dependencies, unrefined work packages, and parallel-agent ownership.

| Task | Status |
| --- | --- |
| [Extract the shared Kubernetes-style resource envelope](extract-shared-resource-envelope.md) | Ready |
| [Migrate and integrate the shared variables subsystem](migrate-and-integrate-variables-subsystem.md) | Ready |
| [Implement the Kagami MCP server](kagami-mcp-server.md) | Ready; later slices gated |
| [Implement the Kagami object catalog](implement-kagami-object-catalog.md) | Core implemented and accepted; integration gated |
| [Fix Kagami catalog authority collision and path-containment boundaries](fix-kagami-catalog-authority-boundaries.md) | Implemented and accepted |
| [Integrate catalog variables and capture them into workloads](capture-catalog-values-in-expressions.md) | Gated on catalog, variables, document, and workload contracts |
| [Define and adopt the shared workload format](define-and-adopt-shared-workload-format.md) | Ready; foundational priority |
| [Implement resumable observation streaming](implement-resumable-observation-streaming.md) | Ready |
| [Implement time-addressable run playback](implement-time-addressable-run-playback.md) | Ready |
| [Implement the sans-IO cluster membership core](implement-membership-model.md) | Implemented and accepted |
| [Fix membership liveness propagation through full-record merge](fix-membership-liveness-gossip-merge.md) | Implemented and accepted |
| [Implement the operational cluster-formation PoC](implement-cluster-formation-poc.md) | In progress; public three-worker introducer handoff passes; recovery and lifecycle/fault conformance pending |
| [Implement worker operational observability](implement-worker-observability.md) | Contract slice ready; formation/runtime instrumentation follows owning services |
| [Document and verify operator observability workflows](document-worker-observability.md) | Planned; ships alongside observability slices |
