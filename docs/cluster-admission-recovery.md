# Inspect and recover an interrupted formation join

Status: **recoverable ACK-loss and issuer-loss stop paths have process evidence;
remaining formation conformance pending**

This procedure uses the current PoC CLI and authenticated worker interfaces.
It recovers a still-pending attempt only through the worker's existing pinned
retry path. Exhausted or unverifiable admission stops for operator review;
there is no command to resurrect an exhausted attempt or clear uncertainty.
It does not prescribe automatic restart, switching introducers or removing an
exclusion. See the [formation task](tasks/implement-cluster-formation-poc.md)
for the remaining acceptance evidence.

## Before doing anything

Keep the original source worker endpoint, its operator credential file and
operation ID. Keep the original target formation and introducer endpoint from
the private join material, plus that introducer's separate operator credential.
The source and issuer operator credentials need not be the same. Never pass
the formation join token as an operator credential or paste private join
material into an incident report.

Use matching worker/CLI builds supporting JoinOperation version 2. No command
below submits a new join, leaves a formation, changes policy or resets a retry
budget. **Stopping this procedure means stopping operator actions, not killing
the worker or cancelling its existing attempt.** The worker can continue its
already-bounded background recovery.

## 1. Establish source state and a finite observation window

Read the original operation from the directly addressed source:

```sh
orishuctl --host SOURCE_SOCKET --operator-token-file SOURCE_OPERATOR_FILE \
  --timeout 5s --output json join-status ORIGINAL_OPERATION_ID
orishuctl --host SOURCE_SOCKET --operator-token-file SOURCE_OPERATOR_FILE \
  --timeout 5s --output json cluster info
```

Check the operation ID and historical source/target identities. Before adoption,
the source's current formation/node must match the recorded source; after
adoption, the formation must match the target and the node must match the
operation's assigned `state.nodeId`. Also check current participation: an
ejected, stopping or subsequently changed lifecycle is not recovered merely
because a historical operation says `joined`. If these disagree, the operation may be
historical: stop instead of acting on it as the current lifecycle.

For this PoC, use an operator observation budget of **300 seconds from the
first poll**, with at least one second between polls and a five-second timeout
per CLI call. Do not extend the deadline on a changed phase, reconnect, failed
read or repeated invocation. The current default retry timer windows are
1, 2, 4, 8, 16, 32, 60 and 60 seconds (183 seconds total); initial handshake
allows 15 seconds and post-adoption catch-up has a 90-second budget. Rebinding
or refusal can consume retries sooner. The 300-second observation cutoff is
operator policy, **not a guaranteed completion deadline or a production SLO**:
scheduling delays or owner failure can exceed it. A timeout means stop and
preserve evidence, never that remote insertion did not happen. If these runtime
limits change, review this budget alongside the protocol and tests.

| Source `state.phase` | Action |
| --- | --- |
| `connecting`, `admitting` | Poll the same operation within the original observation budget. If a reference is present, inspect the original issuer as below. Do not create another operation. |
| `catchingUp`, `catchUpFailed` | The target assignment has been adopted. Keep that identity; allow existing bounded catch-up retries within the same observation budget. Do not roll back to the old standalone formation. |
| `joined` | Verify current target formation/assigned identity and current `joined` participation. The join is complete; stop recovery polling. Introduction additionally depends on configured role and `introducerReady`, not this phase alone. |
| `failedBeforeAdmission` | This recorded attempt failed before admission was emitted. Stop this procedure; a corrected, separately reviewed join requires current standalone eligibility, fresh preconditions and a new operation ID. Do not infer that other attempts or older lifecycles were refused. |
| `unresolved` | Automatic admission recovery has ended without a definitive outcome. Inspect retained evidence, then stop for review even if the issuer reports a current member. This PoC cannot resume an exhausted attempt. |
| Missing operation, unreadable status, invalid/mismatched response or expired observation budget | Stop for review. Missing process history or inability to inspect is not evidence of non-admission. |

If a submission response was lost, exact resubmission of the original request
with the original ID and unchanged private material can recover its record
while the original process retains history. It does not start another dial or
reset timers. Do not reconstruct a changed request, use a new ID, or resubmit
after source restart as a recovery technique.

## 2. Inspect the original issuer's retained evidence

Use `targetFormationId` and all four fields of `recoveryReference` from source
status. A null reference supplies no issuer lookup key; retain the source
status and stop at its terminal phase or the observation deadline. Do not
guess an attempt ID from the operation ID, member ID, worker label or token.

Address the original introducer, not an arbitrary formation member:

```sh
orishuctl --host INTRODUCER_SOCKET --operator-token-file INTRODUCER_OPERATOR_FILE \
  --timeout 5s --output json admission-inspect \
  --formation-id TARGET_FORMATION --attempt-id PEER_ATTEMPT \
  --applicant-fingerprint APPLICANT_PIN --introducer-node-id INTRODUCER_ID \
  --introducer-fingerprint INTRODUCER_PIN
```

Check the echoed request against the source record. For a matching issuer,
`sourceFormationId` and `sourceNodeId` must equal the original target formation
and introducer ID. An endpoint hint does not replace authenticated identity.

| Issuer `outcome.kind` | Meaning and next action |
| --- | --- |
| `currentMember` | The issuer retains the assignment and it passes current local membership checks. If the source attempt is still pending, its existing retry path may recover this same ID. Continue source polling, not a new join. An unresolved source still requires stop/review. |
| `retiredOrRestricted` | Retained assignment cannot be used under current local membership checks. Stop/review; do not resurrect it, remove an exclusion or infer permission for fresh admission. |
| `recordUnavailable` | No accepted record is retained for this exact pair. If source work is still pending, it may not yet have reached the issuer; observe only the same attempt within the original budget. Otherwise stop/review. Absence never proves refusal or absence of a certificate exclusion. |
| `wrongIssuer` | The process is not the original formation/node/certificate issuer, including after its restart. Stop/review; another member cannot substitute for its formation-lifetime ledger. |
| Authentication, transport, timeout or decoding failure | Stop/review. Do not downgrade authentication, disclose the token to another endpoint or treat failure as a negative admission report. |

Reports are local-at-request diagnostics. Even `currentMember` does not prove
current token/source-network authorization, introducer readiness or successful
catch-up. A lock can safely refuse new members while retaining an existing
assignment. No report establishes globally current policy under partition.

## 3. Preserve evidence and escalate without changing authority

Keep secret-free source status, issuer report, current source summary, CLI
exit/error categories, build versions and observation timestamps. Record which
worker was queried and whether either process restarted. Stop automation that
would issue new joins or destructive recovery. Consult the operator responsible
for the original formation and retain the explicit unresolved outcome until a
separately supported, reviewed recovery action exists.

Do not reset private state, rotate a certificate, clear exclusions, restart
all peers or infer safety from an empty `ls` result. This PoC does not offer a
cluster-wide non-insertion proof or durable admission ledger. These are deliberate
stop conditions, not instructions to wait indefinitely or bypass the gates.

## Evidence and limits

The public CLI harness exercises normal handoff, exact status replay, current
and retired issuer reports, absent records, wrong issuer after restart, and
authorization/malformed-input rejection. The real-QUIC runtime lost-ACK test
recovers the original assignment and inspects it at the issuer. The explicit
fault-build harness also passes that recovery and inspection across independent
processes. The full fault/handoff/churn journey has also passed after a
deterministically tested shutdown-supervision race fix. A later reproduced
convergence failure exposed a lost suspicion-notification gap; the task records
the bounded reminder regression, ten passing handoff repetitions and a passing
full lost-ACK/handoff/churn rerun. These do not prove the remaining fault matrix.
Separately,
`make test-formation-issuer-loss` verifies the mandatory unresolved stop journey:
the issuer exits after insertion, the source exhausts its actual retry windows
without adopting or resetting identity, rejects new join/leave work, and retains
the same uncertainty when the restarted issuer reports `wrongIssuer`. Neither
this test nor an issuer report authorizes fresh admission or exclusion removal.
The same harness then injects a source crash, retains its certificate/operator
credential files and verifies fresh standalone identities plus authenticated
`UnknownOperation` for the old request. No new join is submitted. This proves
the missing-history stop condition after both process histories are lost.
The separate `make test-formation-source-loss` journey keeps the original
issuer alive with its retained ledger. Following confirmed insertion and ACK
loss, it pauses that issuer briefly to arrange a pre-adoption source crash,
restarts the source with retained credentials and resumes the issuer. The
source reports fresh standalone identities and `UnknownOperation`; the issuer
still identifies the original assignment, whether locally current or already
retired by liveness. The report does not restore source history or trigger a
new join. This is fault injection, not a suggested recovery procedure.
`make test-formation-dead-assignment` covers a surviving source whose accepted
ID becomes Dead before recovery. The harness pauses the source after confirmed
ACK loss, waits for normal issuer SWIM retirement, then resumes the original
attempt. Repeated public checks require the assignment to remain Dead with no
extra member, and the source to finish unresolved without changing identity or
its recovery reference. Issuer inspection reports `retiredOrRestricted`; exact
request replay cannot reset exhaustion, and fresh join/leave requests refuse.
No operator tombstone or certificate block is injected by this case.
The `--removed-assignment` process journey (reproducible with
`make test-formation-removed-assignment`) applies the existing serialized
force-removal command after insertion, before delivering acceptance. Public
checks require `retiredOrRestricted`, absence of both the removed ID and any
replacement member at the issuer, and bounded exhaustion of the same source
attempt. Exact request replay cannot reset it; a new join or leave refuses.
No new public removal route or operator recovery authority is introduced.
`make test-formation-blocked-assignment` instead blocks the authenticated
applicant fingerprint after insertion and before acceptance delivery. The
issuer retains the member record, but inspection stays `retiredOrRestricted`
throughout the original source's recovery. Exact member/fingerprint checks
exclude a replacement identity; the source reaches bounded exhaustion with
unchanged identity/reference and rejects new join/leave work. No exclusion is
cleared and no certificate is rotated.

The separate `make test-formation-excluded-restart` negative conformance test
restarts that blocked source with retained credentials and verifies fresh
identities and missing operation history. A deliberately submitted fresh
attempt still cannot adopt or allocate another member; it ends unresolved,
while the original issuer retains the old restricted assignment and has no
accepted record for the new attempt. A different certificate can join through
the same issuer/material, ruling out blanket admission failure. This is a
test of exclusion enforcement, **not an exception to this runbook's stop
condition** or a recommended restart/resubmission procedure.
`make test-formation-peer-ejection` separately verifies a fully joined worker
learning its own removal from a test peer over real authenticated gossip. It
retains old identities and historical join status while reporting `ejected`,
withholds join material and stops live membership participation, then handles
explicit leave to fresh standalone state. The test peer sends a valid fixture;
it does not apply a cluster-wide administrative removal. A historical `joined`
receipt therefore remains insufficient evidence of current participation.
Combined fault acceptance stays open in
N-FORMATION. Telemetry may assist diagnosis but never supplies missing admission
authority; metrics, probes and traces remain companion implementation work.
