# Administer kernel and resource artifacts

Status: planned operator stories, not implemented commands. Follow-up:
[artifact cache and admission policy](../../tasks/implement-artifact-cache-administration.md).

Workers receive digest-pinned kernels and inputs through workloads, not installed
Kagami plugins. Administrators manage artifact storage and security policy through
the same public API in `orishuctl` and `orishu-monitor`.

## Inspect what is stored and where

As a cluster administrator, I want to inspect kernels and other workload resources
on each node so I can understand storage use and readiness for an expected workload.

- Filter by node, exact digest, artifact role or workload reference; show byte size,
  verification state, holders, transfer progress, residency claims and active pins.
- Distinguish direct verified node reports from stale/unreachable inventory views.
  An offline node is unknown, not empty, and an advertisement is not verified bytes.
- Show completeness for a supplied manifest and selected target nodes, without
  loading or starting the workload. Cached bytes do not imply ABI, scientific,
  execution-policy or hardware admission success.
- Bound/paginate queries and preserve authorization for potentially sensitive
  workload names, artifact metadata and origin details.

## Pre-position required artifacts

As an administrator, I want to upload kernels/resources from my local machine or
copy them from a verified worker to selected nodes before a workload arrives.

- Address content by digest, not a mutable plugin tag; validate expected size and
  digest before publishing verified availability. Upload/copy never executes code.
- Source and destination are explicit; worker-to-worker transfer need not relay
  through the administrator's machine. Authenticate both the operation and peers.
- Report per-target queued/transferring/verified/failed states and partial completion;
  retries/resume are bounded and idempotent. Unreachable destinations are explicit.
- Inspect retention/reservation status separately from a completed copy. Do not
  promise future residency if ordinary eviction may immediately remove the data.
- Denied content cannot become executable through pre-positioning. Storage policy
  may reject or quarantine it according to the reviewed security policy.

## Reclaim space without damaging runs

As an administrator, I want to remove selected cached artifacts from selected
nodes, with a clear preview of affected bytes, references and active uses.

- Local cache eviction is distinct from logical deletion of stored results/
  checkpoints and from execution denial. Evicted allowed content may be fetched
  again; deletion tombstones and deny rules have different semantics.
- Active runtime, checkpoint or reader pins prevent unsafe deletion. Report blockers
  or explicitly defer reclamation; never silently damage a running workload.
- Show requested versus actually reclaimed bytes, with per-target outcomes and
  offline-node limitations. Logical removal need not mean immediate physical erase.
- Purging bytes alone is not a security remedy: a submitter or peer could supply
  them again unless policy forbids their use.

## Prevent use of compromised content

As an administrator, I want to deny an exact kernel digest so a workload requiring
it cannot start, even if those bytes are cached, uploaded again or held by a peer.

- Support reason, actor and an auditable policy revision; role labels, filenames,
  package names and alternate locations cannot bypass an exact-content rule.
- Apply policy to required transitive executable/input artifacts, not merely the
  root manifest. A digest appearing only in unused release-provenance metadata
  is not automatically an executable dependency.
- Check policy before admission/start and restore/reassignment paths that could
  execute the artifact. Report the offending digest and policy decision clearly.
- Report policy propagation/enforcement status, including stale or offline nodes;
  do not claim cluster-wide protection after changing only one node's local list.
- Define already-running workload handling and an explicit stop/quarantine response
  in the security design. Do not imply that denying future starts terminates code
  already executing. If containment cannot be confirmed, show that limitation.
- Lifting a denial is a separately authorized/audited policy operation, not an
  un-purge or a claim that the artifact is safe. Retention across restarts/formations
  must be explicit before durable protection is advertised.

## Operate safely on a shared cluster

- Distinct permissions cover inventory read, upload/copy, retention, eviction,
  logical purge and policy changes. Ordinary workload submission grants none of
  those administrator powers implicitly.
- Apply quotas, transfer/concurrency/disk limits and bounded diagnostics. Authenticated
  clients and peers remain untrusted; integrity proves content identity, not safety.
- Both operator clients use identical operation IDs, retries, confirmations and
  structured outcomes. Security policy is cluster authority, never a TUI-only filter.
