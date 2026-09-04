# Orishu runtime deferred design work

This ledger preserves reviewed standalone-Orishu proposals that remain outside
the monorepo's accepted MVP behavior. Entries are not implementation authority.
Each requires an ADR or bounded task before implementation.

## Dynamic worker participation

MVP participation flags (`accepts.peers`, `accepts.clients`, `accepts.work`) are
startup configuration. Changing them requires configuration update and restart.

A post-MVP design may allow privileged, idempotent live changes, but the flags
do not share identical semantics:

- disabling peer admission stops new joins without breaking existing peer
  relationships;
- disabling client admission needs an explicit policy for existing sessions;
- disabling work admission requires observable drain and fenced ownership
  transfer at a committed boundary.

Open questions include node-local versus cluster-replicated intent,
mixed-version behavior, pending/applied/rejected status, timeouts and force,
audit events, and deterministic ownership during drain.

## Discovery-assisted admission

MVP formation is explicit: an operator supplies an introducer and join token.
mDNS visibility, an existing certificate, or network reachability does not
authorize automatic admission.

A future mDNS design must remain opt-in and specify advertised identity,
expected-formation selection, token storage/rotation, first join versus rejoin,
multiple visible clusters, stale/flapping announcements, retry bounds, audit
output, malformed multicast input, and mixed-version behavior. Discovery must
not silently choose a plausible cluster.

## Runtime replication changes

MVP `storage.replicas` is startup configuration. A later runtime change must
decide whether it affects future writes, existing artifacts, or both; whether
the requested target is cluster-wide or node-local; how requested and effective
replication are shown; and how insufficient peers are handled.

Increasing a target may require bounded background re-replication. Decreasing
it raises explicit pruning and durability questions. Neither operation may
quietly claim success while historical artifacts remain at another durability
level.

## Client API compaction review

Before implementing the imported client protocol as stable API, resolve these
recommendations:

1. Move checkpoint artifacts from `/cluster/workload/checkpoints` to the
   top-level `/cluster/checkpoints`, symmetric with results and consistent with
   checkpoints outliving the currently loaded workload.
2. Replace verb-shaped `POST /cluster/workload/check` with a noun resource such
   as a compatibility report.
3. Rename or explicitly scope `/cluster/tombstones` as membership tombstones so
   it cannot be confused with artifact purge tombstones.
4. Keep events, logs, and audit records separate, but define their authority,
   retention, and overlap precisely.
5. Keep `GET /cluster` as a formation summary rather than duplicating the node
   collection.

These are proposals, not silently adopted path changes. Decide them before
shipping compatible clients because the first three affect API shape.

## Storage questions

The open contracts in `storage-spec.md` include artifact commit transitions,
hinted handoff, re-replication, purge retention/compaction, external-backend
deletion, and bounded whole-artifact delivery.

Dynamic engine capability advertisement is also deferred. Nodes advertise only
security contracts they currently enforce; live capability renegotiation needs
an explicit epoch/version and workload-safety design.

The client protocol supports optimistic concurrency. Making `orishuctl` discover
and send validators by default remains a separate operator-UX decision.

## Distribution ideas not automatically retained

The historical backlog mentioned Snap and Nix packaging in addition to native
packages, Homebrew, Cargo installation, archives, and containers. The current
roadmap retains the latter set. Snap and Nix should be treated as uncommitted
ideas unless product support and maintenance ownership are explicitly restored.
