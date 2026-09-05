# Fix membership liveness propagation through full-record merge

Status: **implemented and accepted — N-MEMBERSHIP acceptance unblocked**

Parent task:
[Implement the sans-IO cluster membership core](implement-membership-model.md)

Design: [peer protocol version and merge semantics](../protocol-p2p.md#versiontuple)
and [`merge.rs`](../../crates/orishu-membership/src/merge.rs)

## Outcome

A liveness change learned through a direct SWIM `Announce` continues to
propagate when the resulting full `MembershipUpdate` record is carried through
gossip or anti-entropy. Member description and SWIM liveness retain their
independent orderings:

- `VersionTuple` orders descriptive fields; and
- `(Liveness, Incarnation)` follows the SWIM override table.

An equal descriptive version is not a conflict merely because liveness or
incarnation differs. Conversely, genuinely different descriptive payloads at
the same version remain observable conflicts and are never resolved by arrival
order.

Completing this task unblocks acceptance of N-MEMBERSHIP and assignment of
[N-FORMATION](implement-cluster-formation-poc.md).

## Defect

`apply_announcement` correctly changes only liveness/incarnation and queues the
locally merged full member record without incrementing its descriptive
`VersionTuple`. On the receiving path, however, `merge_member` currently treats
any non-identical member record at the same version as `VersionConflict` before
calling `swim_supersedes`.

For three nodes A, B, and C:

```text
A receives Announce(Suspect, C, incarnation n)
        |
        | A adopts it and queues full MembershipUpdate(C)
        v
B receives that update through gossip or anti-entropy
        |
        | current defect: equal VersionTuple => VersionConflict
        v
B incorrectly keeps C Alive instead of applying SWIM ordering
```

The existing direct-announcement and descriptive-conflict tests both pass, but
no test carries an announcement-derived record across the next dissemination
hop.

## Required semantics

Treat the member record as two independently ordered projections after the
existing validation, tombstone, self-identity, unknown-member, and certificate
binding checks:

| Projection | Fields | Ordering |
| --- | --- | --- |
| Description | ID, label, pinned certificate, protocol, endpoints, admission flags, capacity, capabilities | `VersionTuple` |
| Liveness | state and incarnation | SWIM override table |

The merge decision is:

1. Exact full-record replay remains `Idempotent`.
2. A newer descriptive version adopts the incoming description; an older one
   retains the held description.
3. Equal versions with equal descriptive fields are not a conflict. Liveness
   is considered independently.
4. Equal versions with different descriptive fields retain the held
   description and emit `Diagnostic::VersionConflict`.
5. In every case after the hard identity/certificate/tombstone checks,
   independently apply incoming liveness when `swim_supersedes` says it wins.
6. If liveness wins while the description conflicts, apply and re-gossip the
   locally merged record while also reporting the descriptive conflict. Do not
   drop valid failure-detector information because an unrelated description is
   conflicted, and never re-gossip the conflicting incoming description.
7. If neither projection changes, return the appropriate idempotent, stale, or
   conflict result without enqueueing or publishing a false change.

The exact internal representation is not prescribed. Extending `MergeOutcome`
to carry both an adopted value and optional diagnostics is reasonable, but a
different small representation is acceptable if callers cannot accidentally
discard either result.

Certificate mismatch remains a hard rejection before these independent
decisions. Tombstones continue to outrank every liveness/version claim. The
special self-challenge/refutation path must remain intact.

## Implementation steps

1. Add a private, explicit comparison for the descriptive projection. Avoid a
   fragile sequence of field comparisons duplicated across tests and merge
   branches; a helper or private projection is preferable.
2. Refactor `merge_member` so equal-version conflict detection concerns the
   descriptive projection rather than the complete record.
3. Allow one merge to return both a locally merged record/change and a
   structured descriptive conflict when both occur.
4. Update `absorb_outcome` so it:
   - enqueues only the locally held merged record;
   - publishes a liveness change exactly once when liveness changed; and
   - preserves any accompanying diagnostic.
5. Clarify `protocol-p2p.md` under `VersionTuple`: the generic equal-version
   rule applies to the version-owned entity projection; `NodeRecord` liveness
   is separately ordered by the SWIM table. No wire shape or version bump is
   expected.
6. Add the regression and matrix coverage below, then run the parent task's
   complete acceptance suite.

## Required tests

### Propagation regression

- Build three logical member views. Deliver `Announce(Suspect)` about C to A,
  then deliver A's queued full record to B as piggybacked gossip. B adopts
  `Suspected` at the same descriptive version.
- Repeat through a real `PullRequest`/`PullReply` anti-entropy exchange rather
  than calling a private merge helper.
- Cover `Dead(n)` propagation at an equal descriptive version and
  `Alive(n + 1)` refutation propagation at a strictly newer incarnation.

### Independent-ordering matrix

- Equal description version + equal description + winning liveness: liveness
  changes, no `VersionConflict`.
- Equal description version + equal description + stale liveness: no state
  change and no false conflict.
- Equal description version + conflicting description + winning liveness:
  held description remains, liveness changes, one conflict diagnostic is
  reported, and the queued record equals local state.
- Equal description version + conflicting description + non-winning liveness:
  held state remains and one conflict diagnostic is reported.
- Newer description + non-winning liveness: description changes while current
  liveness remains, preserving the existing regression test.
- Older description + winning liveness: description remains while liveness
  changes.
- Exact replay remains idempotent and does not reset gossip hops.

### Safety regressions

- A mismatched certificate cannot smuggle in a liveness update.
- A tombstoned identity cannot be resurrected by any version/incarnation.
- An incoming record about the local node still takes the self-challenge and
  refutation path rather than overwriting local identity/state.
- Duplicate and reordered non-conflicting description/liveness deliveries
  converge to the same final model. Effects remain bounded and diagnostics
  remain semantically appropriate; their chronological sequence need not be
  byte-identical across different delivery orders.

Prefer public `update`-level tests in `tests/convergence.rs`; private unit tests
may supplement them but must not be the only proof of gossip and anti-entropy
behavior.

## Acceptance criteria

- The propagation regression fails before the correction and passes after it.
- Gossip and anti-entropy apply the same member merge semantics.
- Description conflicts remain deterministic and visible while independently
  newer liveness still converges.
- Every adopted delta queued for further gossip is the locally held merged
  record, never an incoming partially rejected record.
- Existing SWIM, admission, join, removal/tombstone, bounds, wire-fixture, and
  dependency-purity behavior remains unchanged.
- No networking, runtime, clock, filesystem, TLS, RNG, or new credential
  capability enters `orishu-membership`.
- `docs/tasks/implement-membership-model.md` and the roadmap mark
  N-MEMBERSHIP accepted only after this task and the complete parent acceptance
  suite pass.
- These commands pass:

  ```sh
  cargo fmt --all -- --check
  cargo clippy --locked -p orishu-identity -p orishu-membership --all-targets -- -D warnings
  cargo test --locked -p orishu-identity -p orishu-membership --all-targets
  make docs-check
  ```

## Non-goals

- Changing the SWIM override table or incrementing `VersionTuple` for liveness.
- Changing wire field names, CBOR/JSON shapes, protocol version, Merkle
  canonicalization, gossip retirement, or anti-entropy bounds.
- Adding cluster-wide membership policy, worker configuration, QUIC/mTLS,
  codecs, timers, entropy, persistence, or client APIs; those belong to
  N-FORMATION.
- Redesigning admission, join snapshots, removal/tombstone semantics,
  blocklists, identity allocation, or certificate rotation.
- Broad cleanup or performance work unrelated to the failing merge path.
