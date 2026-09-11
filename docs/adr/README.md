# Architecture decision records

ADRs record decisions that are costly to reverse or easy to accidentally
rediscover. Use a four-digit sequence and include context, decision,
consequences, and status.

- [0001 — Product-oriented monorepo](0001-product-oriented-monorepo.md)
- [0002 — Scope orishu to initial value problems](0002-initial-value-problem-scope.md)
- [0003 — Distribute orishu as a single self-sufficient binary](0003-single-binary-distribution.md)
- [0004 — Separate authoring commands from run observations](0004-separate-authoring-commands-from-run-observations.md)
- [0005 — Author numeric values as unit-aware expressions](0005-author-numeric-values-as-unit-aware-expressions.md)
- [0006 — MCP and the UI are equivalent interfaces to one session](0006-mcp-ui-equivalence.md)
- [0007 — Share expression semantics with workload resources](0007-share-expression-semantics-with-workload-resources.md)
- [0008 — Catalog templates instantiate self-contained experiment objects](0008-catalog-templates-instantiate-self-contained-objects.md)
- [0009 — Execute workloads as sandboxed portable programs](0009-execute-workloads-as-sandboxed-portable-programs.md)
- [0010 — Use content-addressed workload closures and portable bundles](0010-content-addressed-workload-closure-and-portable-bundles.md)
- [0011 — Classify network flows and baseline observation deltas](0011-classify-network-flows-and-baseline-observation-deltas.md)
- [0012 — Start with file sharing and preserve collaborative authoring](0012-start-with-file-sharing-and-preserve-collaborative-authoring.md)
- [0013 — Keep cluster formation and node membership identity distinct from labels](0013-cluster-formation-and-node-identity.md)
- [0014 — Prevent purged artifacts from returning through stale inventory](0014-prevent-purged-artifact-resurrection.md)
- [0015 — Use QUIC-native transfer for committed artifact chunks](0015-use-quic-native-artifact-transfer.md)
- [0016 — Use a narrow Maxwell/Yee profile for first distributed conformance](0016-first-distributed-scientific-profile.md) (**proposed for revalidation**)
- [0017 — Expose bounded worker metrics, traces and health probes](0017-worker-operational-observability.md) (**accepted; formation-stage implementation delivered, combined M4 acceptance pending**)
- [0018 — Catalog template properties are authoring variables captured into workloads](0018-catalog-values-are-captured-by-reference-not-linked.md) (**proposed**)
- [0019 — The experiment document is a validated model behind one document server](0019-kagami-experiment-document-model.md)
- [0020 — Compose object behaviour through plugin components](0020-compose-object-behaviour-through-plugin-components.md)
- [0021 — Capture particle-emitter recipes in workloads](0021-capture-particle-emitter-recipes-in-workloads.md)
- [0022 — Persist a default view outside experiment intent](0022-persist-default-view-outside-experiment-intent.md)
- [0023 — Fields are plugin-modelled state over the experiment domain](0023-fields-are-plugin-modelled-domain-state.md)
- [0024 — Orishu orchestrates a workload component graph](0024-orishu-orchestrates-a-workload-component-graph.md)
- [0025 — Version peer trace-context propagation separately from membership semantics](0025-version-peer-trace-context-propagation.md) (**accepted; formation admission propagation implemented**)
- [0026 — Bind each network role's sockets to one explicitly named interface](0026-worker-network-interface-placement.md) (**accepted; one interface per role, Linux-only PoC enforcement**)
