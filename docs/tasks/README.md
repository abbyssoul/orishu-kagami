# Tracked tasks

Tasks refine accepted product and architecture decisions into bounded,
verifiable implementation work. `TODO.md` remains an inbox for unrefined ideas;
once promoted, an idea links to a task here.

See the [implementation roadmap](../roadmap/README.md) for milestone order,
cross-task dependencies, unrefined work packages, and parallel-agent ownership.

Kagami's capability work is a multi-task programme with its own index:
[docs/tasks/kagami](kagami/README.md) carries the authoring spine and its
cross-lane work — the experiment
model, the document server every authoring adapter commands, persistence,
catalog instantiation, composed execution, observation instruments, emitters,
and viewport/app adoption.

| Task | Status |
| --- | --- |
| [Kagami capability programme](kagami/README.md) (K1–K13) | K1, K3 and the landed-boundary follow-up implemented; K7 partially implemented; K2, K4–K6, K8–K13 specified |
| [Extract the shared Kubernetes-style resource envelope](extract-shared-resource-envelope.md) | Ready |
| [Migrate and integrate the shared variables subsystem](migrate-and-integrate-variables-subsystem.md) | Ready |
| [Implement the Kagami MCP server](kagami-mcp-server.md) | Ready; later slices gated |
| [Implement the Kagami object catalog](implement-kagami-object-catalog.md) | Core implemented and accepted; integration gated |
| [Fix Kagami catalog authority collision and path-containment boundaries](fix-kagami-catalog-authority-boundaries.md) | Implemented and accepted |
| [Integrate catalog variables and capture them into workloads](capture-catalog-values-in-expressions.md) | Gated on catalog, variables, document, and workload contracts |
| [Define and adopt the shared workload format](define-and-adopt-shared-workload-format.md) | Ready; foundational priority |
| [Implement the WebAssembly component-graph host](implement-wasm-component-graph-host.md) | Specified; gated on the S-WORKLOAD graph and component ABI |
| [Publish installable Orishu Kagami artifacts](publish-installable-artifacts.md) | In progress: candidate artifacts and gates implemented; supported publication gated by M8 |
| [Implement resumable observation streaming](implement-resumable-observation-streaming.md) | Ready |
| [Implement time-addressable run playback](implement-time-addressable-run-playback.md) | Ready |
| [Implement the sans-IO cluster membership core](implement-membership-model.md) | Implemented and accepted |
| [Fix membership liveness propagation through full-record merge](fix-membership-liveness-gossip-merge.md) | Implemented and accepted |
| [Implement the operational cluster-formation PoC](implement-cluster-formation-poc.md) | In progress; public three-worker introducer handoff passes; recovery and lifecycle/fault conformance pending |
| [Implement worker operational observability](implement-worker-observability.md) | Initial local probes/health gauges documented; full probe/security matrix, formation metrics, traces and acceptance pending |
| [Document and verify operator observability workflows](document-worker-observability.md) | Local source-build scrape recipe verified; remaining deployment/security/trace/alert handoff pending |

The [cluster-formation integration record](cluster-formation-integration-record.md)
preserves historical implementation reports; it is not another task or a
current acceptance checklist. Use N-FORMATION's
[audit selection and closure rules](implement-cluster-formation-poc.md#selecting-and-closing-an-audit-increment),
remaining increments and failure matrix to select its next work. Its
[closure gates](implement-cluster-formation-poc.md#acceptance-criteria) distinguish
formation acceptance from the combined M4 telemetry/operator handoff. Its
[recovery completion checklist](implement-cluster-formation-poc.md#interrupted-admission-recovery-completion-checklist)
audits recovery conformance around the recorded interfaces; it does not
reimplement correlation or issuer inspection. The
[current conformance ledger](cluster-formation-conformance.md) maps the failure
matrix to inspected evidence. A controlled timing fixture has corrected the
readmission observation budget; retain it in final process reruns and use the
[conditional bounded investigation](implement-cluster-formation-poc.md#conditional-investigation-a-new-readmission-failure)
for new failures. The remaining formation conformance/recovery audit can proceed. The
next uncovered transport work is HTTP/2 incomplete-header bounds and the
remaining flow-control audit (inbound violations and withheld-credit deadline
semantics). Response stream/connection credit exhaustion and explicit recovery
now have [real-process evidence](cluster-formation-conformance.md#http2-response-credit-exhaustion-and-recovery--2026-09-06).
Continuation count and stream binding also have a real-worker regression;
incomplete header/frame timing remains distinct from those rejection cases.
Silent clients now have a tested ten-second connection-idle deadline; the
remaining temporal audit concerns active trickle/interleaved traffic, not
reimplementing silent-client expiry.
Stream-capacity and decoded header enforcement have real-wire evidence;
the mutation credential matrices now pass for Unix and certificate-verified TLS
under both HTTP/1.1 and HTTP/2. See the ledger's
[authority follow-up](cluster-formation-conformance.md#mutation-authority-matrix-and-client-error-status--2026-09-06).
The
[retained unfinished-client IO acceptance contract](implement-cluster-formation-poc.md#retained-acceptance-contract-unfinished-client-io)
now has scoped evidence for slow response writes as well as header and shutdown
cases; preserve their limits and audit the mapping. Incomplete-header
admission/expiry has real-worker
evidence alongside peer progress under sixty held headers. Shutdown with
unfinished clients now has process evidence after fixing competing drain
requests. Preserve the recorded peer saturation, slow-body capacity recovery,
datagram replay/probe and formation/lifecycle evidence; final audits and combined
M4 observability acceptance remain open.

The [stale-IO completion inventory](cluster-formation-conformance.md#stale-io-completion-inventory--2026-09-06)
maps actual asynchronous classes separately from inline owner effects. Old
join-preparation cancellation now has a lifecycle regression.
[Held successful initial handshakes](cluster-formation-conformance.md#late-successful-initial-handshake-and-shutdown-disposal--2026-09-06)
also have leave/shutdown disposal evidence after fixing post-shutdown reserved
completion retention; reliable-send completion mapping remains an audit gap.
The [send-result consumer regression](cluster-formation-conformance.md#reliable-send-result-consumption-across-generations--2026-09-06)
now verifies old successful/error results cannot affect replacement state or
counters; transport completion and in-flight cancellation remain distinct.
[Owner timer cleanup](cluster-formation-conformance.md#owner-timer-cleanup-on-leave--2026-09-06)
also has direct pending-deadline and late-token leave coverage, separate from
the core's stale-token tests.
The [post-fix process reruns](cluster-formation-conformance.md#process-regression-reruns-after-completion-disposal--2026-09-06)
pass full lost-ACK, client-pressure and policy-partition journeys. Formatting
and documentation checks have recovered from the concurrent scaffold gaps;
these checks do not close the outstanding acceptance rows.

The [current workspace validation](cluster-formation-conformance.md#current-workspace-validation--2026-09-06)
passes default all-target/doc tests, Clippy and formatting after the idle-deadline
change. Documentation initially passed; the final recheck failed on eight
missing task links in a concurrently added Kagami index. It is an integration baseline, not completion
of the remaining conformance, separate fault-process or observability gates.
