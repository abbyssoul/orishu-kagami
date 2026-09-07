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
| [Kagami capability programme](kagami/README.md) (K1–K13) | K1, K3, the landed-boundary follow-up, and K4 experiment persistence implemented; K2, K5, and K6 have implemented slices with named gates; K7 partial; K8–K13 specified |
| [Extract the shared Kubernetes-style resource envelope](extract-shared-resource-envelope.md) | Implemented and accepted; discriminator bounds precede allocation, both no-status policies are documented and tested, and consumer wire compatibility is pinned |
| [Migrate and integrate the shared variables subsystem](migrate-and-integrate-variables-subsystem.md) | Partial: generic and dimensioned evaluation plus experiment integration landed; shared-engine bounds and workload integration remain |
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
| [Implement the operational cluster-formation PoC](implement-cluster-formation-poc.md) | N-FORMATION accepted for source-built Linux; combined M4 telemetry/operator handoff remains open |
| [Implement worker operational observability](implement-worker-observability.md) | Local probes/metrics, OTLP receipt and live trace counters have scoped evidence; log correlation, cross-peer tracing and full probe/security/operator M4 acceptance remain open |
| [Document and verify operator observability workflows](document-worker-observability.md) | Local/mTLS-proxy scrape and pinned local Collector receipt recipes verified; remaining deployment/security/correlation/alert handoff pending |
| [Implement the `orishu-monitor` TUI shell](implement-orishu-monitor-admin-tui.md) | Implemented; first slice toward full operator parity, with live APIs/actions deferred and raw admission-secret output CLI-only |
| [Integrate `orishu-monitor` with the operator API](integrate-orishu-monitor-operator-api.md) | Backlog; promote bounded increments as N-FORMATION and O-CLIENT contracts land |

The [cluster-formation integration record](cluster-formation-integration-record.md)
preserves historical implementation reports and dated lifecycle/recovery
checkpoints; it is not another task or a
current acceptance checklist. Select remaining M4 work through the task's
[open-work table](implement-cluster-formation-poc.md#open-m4-work-selection)
and [bounded increment rules](implement-cluster-formation-poc.md#selecting-and-closing-an-audit-increment).
The following formation summaries preserve historical checkpoints, not a new
implementation backlog. The task's
[assembly-deadline increment](implement-cluster-formation-poc.md#http2-assembly-deadline-increment)
now has a bounded watchdog and real Unix/TLS expiry evidence; the later
[ingress closure](cluster-formation-conformance.md#ingress-pressure-case-to-contract-closure--2026-09-07)
maps the completed pressure audit. Its
[closure gates](implement-cluster-formation-poc.md#acceptance-criteria) distinguish
formation acceptance from the combined M4 telemetry/operator handoff. Its
[recovery completion checklist](implement-cluster-formation-poc.md#interrupted-admission-recovery-completion-checklist)
audits recovery conformance around the recorded interfaces; it does not
reimplement correlation or issuer inspection. The
[current conformance ledger](cluster-formation-conformance.md) maps the failure
matrix to inspected evidence. A controlled timing fixture has corrected the
readmission observation budget; retain it in final process reruns and use the
[conditional bounded investigation](implement-cluster-formation-poc.md#conditional-investigation-a-new-readmission-failure)
for new failures. The
[final acceptance disposition](cluster-formation-conformance.md#final-formation-acceptance-disposition--2026-09-07)
now accepts N-FORMATION, including non-matrix criteria and final process
validation. Remaining implementation is the M4 telemetry/operator handoff. The
HTTP/2 response-credit timing gap now has an
[absolute delivery watchdog and expiry/cancellation evidence](cluster-formation-conformance.md#http2-response-delivery-deadlines--2026-09-07).
Final formation limits and lifecycle/recovery audits have passed at the
boundaries recorded in the ledger.
[Inbound DATA and window-update boundaries](cluster-formation-conformance.md#inbound-http2-flow-control-boundaries--2026-09-07)
now have real Unix wire/server evidence; the isolated stream-overrun fixture
explicitly increases only its connection window, not production defaults.
Response stream/connection credit exhaustion and explicit recovery
now have [real-process evidence](cluster-formation-conformance.md#http2-response-credit-exhaustion-and-recovery--2026-09-06).
Continuation count and stream binding also have a real-worker regression;
incomplete header/frame timing remains distinct from those rejection cases.
Silent clients have a tested ten-second connection-idle deadline; the
[absolute assembly watchdog](cluster-formation-conformance.md#absolute-http2-assembly-watchdog--2026-09-07)
now expires active partial frames and continued header blocks on Unix/TLS.
Response delivery uses the separate deadline documented in the follow-up above.
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
datagram replay/probe and formation/lifecycle evidence. Final formation audits
are accepted at the linked checkpoint; combined M4 observability remains open.

The [stale-IO completion inventory](cluster-formation-conformance.md#stale-io-completion-inventory--2026-09-06)
maps actual asynchronous classes separately from inline owner effects. Old
join-preparation cancellation now has a lifecycle regression.
[Held successful initial handshakes](cluster-formation-conformance.md#late-successful-initial-handshake-and-shutdown-disposal--2026-09-06)
also have leave/shutdown disposal evidence after fixing post-shutdown reserved
completion retention; reliable-send completion mapping was an audit gap at
that historical checkpoint, subsequently reconciled in the final disposition.
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
