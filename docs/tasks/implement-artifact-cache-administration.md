# Implement artifact-cache administration and admission policy

Work package: **O-ARTIFACT-ADMIN**. Status: **backlog; security/protocol design gates
before implementation**. This follows workload artifact delivery, not Kagami plugin
installation. It does not reopen X-PLUGIN's accepted local-package MVP scope.

Stories: [kernel and resource administration](../user-stories/orishu/artifact-administration.md).
Dependencies: O-STORAGE, O-CLIENT/O-API-SHAPE, O-RUNTIME, N-TRANSFER, N-PURGE,
P-MONITOR and S-WORKLOAD. Foundations may be specified in parallel; operational
integration must follow the actual artifact-store and workload admission APIs.

## Outcome and current gap

Operators inspect node artifact residency, pre-position verified kernels/inputs,
reclaim cache space safely and prevent execution of denied content through one
authenticated API with CLI/TUI parity. No worker-side plugin registry, solver
selection or package enablement is introduced.

The workload draft explains selected kernel closure but does not implement these
operator operations. The roadmap still gates O-STORAGE, N-TRANSFER and O-CLIENT.
[Storage specification](../storage-spec.md) and
[ADR 0014](../adr/0014-prevent-purged-artifact-resurrection.md) describe stored-artifact
deletion intent; their `(artifactId, originFormationId)` tombstones are not a
content-digest execution denylist. [ADR 0015](../adr/0015-use-quic-native-artifact-transfer.md)
selects committed-chunk peer transfer, not a complete administrative kernel-upload
API. Inspect real store/client/runtime code before promoting implementation slices.

## Controls that must remain distinct

| Control | Meaning |
| --- | --- |
| Inventory | What a node reports, verified state and freshness |
| Pre-position | Transfer/verify bytes on selected nodes without executing them |
| Retention | Reserve residency under explicit quota/lifetime policy |
| Cache eviction | Reclaim selected local copies; later refetch may be allowed |
| Logical artifact purge | Delete stored resource identity with ADR 0014 suppression |
| Digest denial | Forbid use as an executable/required input regardless of location |

## Slice 0 — Required design decisions

1. Define versioned artifact inventory, operation and policy resources, shared typed
   outcomes, permission scopes, audit events, idempotency and bounded pagination.
   Distinguish per-node local state from collectively accepted policy. Reuse artifact
   digests/node IDs; do not confuse digest, result UID or plugin identity.
2. Specify policy authority and convergence: accepted revision, admission/start
   race handling, offline/stale-node fencing, join/rejoin, restore and partition
   reassignment. Choose fail-closed behavior and prove it before claiming cluster-wide
   denial. Do not mutate the immutable workload to reflect administrative policy.
3. Decide deny-rule persistence across process restart and formations, lifting rules,
   trusted policy distribution and safe history compaction. Existing node blocklists
   and purge tombstones are not this contract. Record a security ADR.
4. Decide response for an already-running workload when one of its dependencies is
   denied: explicit safe stop/containment semantics, candidate rollback and operator
   acknowledgement. No silent scientific substitution or continuation claim while
   some workers are unreachable. A deny action is not proof a compromised host is
   remediated; forensic retention/quarantine and credential response may be separate.
5. Specify upload/copy protocol, same-digest verification, target selection, quotas,
   source authorization, progress, retries/cancellation and retention leases. Define
   denied-content storage behavior without conflating storage with execution authority.
6. Specify deletion previews, active-use pin handling, deferred reclamation, per-node
   completion and separation from reliable checkpoint/result durability requirements.

These choices remain open; the user stories are accepted needs, not acceptance of
a particular consensus mechanism or policy persistence design.

## Bounded implementation slices after design approval

1. Read-only inventory/usage and manifest readiness on selected nodes; preserve stale,
   unknown, claimed and verified distinctions. Add CLI and matching TUI views.
2. Authenticated local upload and worker-to-worker pre-position using existing verified
   artifact storage/transfer primitives, with explicit optional retention and quotas.
3. Exact-target cache eviction and logical purge adapters; enforce runtime/reader pins
   and ADR 0014 where applicable. Expose preview and actual reclaimed-byte outcomes.
4. Digest admission policy core and every runtime enforcement path; persist/propagate
   accepted rules under the approved security design. Denial changes are auditable.
5. Full operator parity and adversarial end-to-end tests, including partial failures,
   restart/rejoin and the agreed active-run containment behavior.

## Acceptance evidence

- Preload an artifact before manifest submission, then prove the worker uses verified
  resident bytes without redundant transfer. No preload operation instantiates a kernel.
- Corrupt/oversized/truncated uploads and false peer availability claims cannot enter
  verified storage; bounded retries cannot create disk/queue exhaustion.
- Copy/evict one selected node without silently affecting other replicas. Test active
  pins, retained snapshots and correct checkpoint/result durability behavior.
- A denied required digest prevents start even from cache, re-upload, another peer,
  resume and reassignment. Test policy-change/admission races and stale/offline nodes.
- Unselected artifact descriptors in release evidence are not treated as required
  executable closure. Renaming media/role labels cannot disguise executing denied bytes.
- Cache eviction alone neither creates nor lifts a deny rule; deny-rule removal does
  not erase purge tombstones. Restart/formations obey the explicitly approved lifetime.
- CLI/TUI use the real serialized API and identical permissions/outcomes; unauthenticated
  or ordinary workload clients cannot invoke privileged writes. Audit contains actor,
  exact targets, policy/operation revision and outcome without credentials.

Record focused tests, serialized integration tests, relevant workspace checks and
`make docs-check`; report manual TUI checks separately. Never close the task merely
because inventory works while denial remains a client-side filter.

## Non-goals

Plugin marketplace or worker plugin installation; automatic trust from content hashes;
signature infrastructure without a separate decision; unrestricted host file access;
automatic forensic erasure; scheduler/job queues; weakening sandboxing for preloaded
code; or guaranteed durable cross-formation purge without its existing design gate.
