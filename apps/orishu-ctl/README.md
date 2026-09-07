# orishuctl

`token` explicitly exports secret join material, not just a token string. It
requires `--operator-token-file` even for a local Unix socket, and the worker
needs an explicitly configured peer listener. JSON/YAML includes the formation,
introducer identity, certificate pin, advertised endpoints and readiness flag.
Transfer it privately; do not paste it into logs, tickets or shell arguments.
Ordinary `ls`, `inspect` and `cluster info` output excludes the token. Current
readiness reflects configured role and completed admission-state/credential
initialization; export does not enable admission. `token --rotate` is explicitly unsupported.

The prepared join input is `join --join-material-file /private/join.json
--formation-id SOURCE_FORMATION --operation-id ATTEMPT_ID`. Export material with
`token --output json` into a private file and transfer it over a trusted private
channel. The loader requires a current-user-owned, non-public, singly linked
regular file, refuses a final symlink/FIFO, and reads at most 16 KiB plus one
overflow byte. Only versioned JSON with bounded literal unicast peer socket
addresses and nonzero ports is accepted. YAML/table output is not join input.
The source formation is the worker addressed by `--host`; the target formation
comes from the material. Do not substitute one for the other. The former raw
`--token` argument and automatic-leave promise have been removed. Submission
now returns a versioned processing record; exit success is not join completion.
Use `join-status ATTEMPT_ID` with the same `--host` and operator credential to
poll, with a bounded deadline in automation. Exact resubmission recovers the
same record without starting another dial; changed input under that ID rejects.
Check `state.phase`: only `joined` denotes completed adoption and catch-up;
`connecting`, `admitting` and `catchingUp` are pending, and `unresolved` is not
a clean rejection. Catch-up is automatically scheduled after adoption; a ready
joined introducer can admit another worker. `catchUpFailed` remains degraded
target-formation state; bounded automatic retries may return it to `catchingUp`.
Lost-ACK recovery can return the original still-live assignment from the same
introducer. Retirement, issuer loss and unavailable history use the tested
bounded stop procedure, not automatic restoration; do not retry with a new ID
after an unresolved outcome.
Version-2 join status includes `recoveryReference` with the peer attempt ID,
applicant fingerprint and original introducer ID/fingerprint. Preserve this
secret-free correlation when reporting an unresolved operation; it is not a
join token, assigned member ID or permission for a new admission. Null or lost
process history does not prove non-admission. Use matching rebuilt worker and
CLI versions. To query retained issuer evidence, address the **original
introducer** with `--host` and its operator-token file, then use:

```sh
orishuctl --host INTRODUCER_SOCKET --operator-token-file OPERATOR_FILE --output json \
  admission-inspect --formation-id TARGET_FORMATION --attempt-id PEER_ATTEMPT \
  --applicant-fingerprint APPLICANT_PIN --introducer-node-id INTRODUCER_ID \
  --introducer-fingerprint INTRODUCER_PIN
```

Copy the target formation and all reference fields from the original join
status; none of these arguments is a secret. This read-only query returns
`currentMember` or `retiredOrRestricted` with the retained assigned ID,
`recordUnavailable`, or `wrongIssuer`. Exit success means the query completed,
not that admission succeeded or another attempt is safe. A current member
report still does not prove replay authorization or catch-up readiness.
Missing history, wrong issuer, restrictions or an unreachable issuer require
stopping and preserving the evidence; do not switch introducers, remove an
exclusion or restart/retry based on the report. A complete bounded recovery
procedure is documented in the [admission recovery runbook](../../docs/cluster-admission-recovery.md);
its mapped recovery process cases and final source-built Linux formation
acceptance now pass. Combined M4 telemetry/operator acceptance remains open.
While the same attempt remains pending, the worker can now redial a lost
connection to the original pinned introducer under bounded retry limits.
Continue polling the existing operation ID; transport redial is not permission
to submit a new join or proof that an earlier remote insertion was recovered.
The peer admission path now refuses `alreadyAdmitted` when the introducer
already knows an alive/suspected member with that certificate. Closing a
connection or restarting a requester does not authorize another live identity.
Dead history alone permits fresh admission, subject to all current exclusion
and policy gates. This refusal is distinct from replay of an accepted attempt;
the CLI can still report unresolved when replay is unavailable or refused.

Leave the directly addressed worker with `leave --formation-id SOURCE
--operation-id ID`, using its operator-token file. This now submits the identified
`POST /api/v1/membership/leaves` resource, not the imported empty DELETE request.
An accepted departure creates fresh standalone formation/node IDs and a join
token, retains the certificate/operator credential and reuses the display label.
The receipt includes previous identities and the summary at acceptance; exact
retry returns that historical receipt without leaving again. A standalone
formation of one is a recorded no-op. The process retains 64 successful leave
receipts without eviction; changed input under a retained ID rejects.

If a leave response is lost, retry the same operation ID and original formation
precondition against the same still-running worker; do not replace them with
its new standalone identities. Inspect `cluster info` separately for current
state. `make test-formation-lost-leave-response` verifies this with an actual
CLI request whose accepted response is discarded by a bounded local proxy.
The replay returns the original receipt without another departure, including
after later readmission. This does not recover receipt history after restart
or prove that every remote peer has already observed the departure.
Poll survivors separately for departure visibility: announcement delivery is
best-effort, with SWIM fallback. Membership lock does not prohibit voluntary
leave. Joining/unresolved admission refuses leave until its outcome is resolved;
do not use restart or repeated new IDs as an ambiguity-recovery procedure.
Compute drain and artifact transfer are not implemented by this PoC.

The formation subset now supports `inspect NODE_ID` using the worker's real
local membership view. Its default is `--source indirect`; direct/best-effort
are explicitly unsupported for now. Output separates assigned identity from
worker label and includes formation, source, liveness, certificate binding,
record version and advertised endpoints/roles. It is not a reachability check.
JSON/YAML inspection output is the versioned resource, not the former flattened
manifest/statistics fields. `ls` collects bounded, identity-checked pages of the
same resource. Name filters are exact; state filters are case-insensitive.
Role filters use advertised acceptance flags, not readiness. Paging is a series
of local reads, not one frozen snapshot; concurrent changes can require a rerun.
The client rejects source/formation changes, non-advancing cursors, more than
4,096 records or 16 MiB of endpoint text, and listings exceeding 30 seconds.

`orishuctl` is the scriptable operator client for an Orishu cluster. Its current
worker-backed surface is the formation subset below. Imported workload,
checkpoint, result, log and audit commands describe later capabilities; their
presence in CLI help is not supported worker behavior. Experiment authoring
and scientific visualization belong in Kagami.

| Current formation support | Boundary |
| --- | --- |
| `cluster info`, `ls`, `inspect` | Real local membership projection; `inspect` supports indirect source only, not host/resource telemetry or a direct reachability probe |
| `token` | Explicit privileged export of pinned join material; no rotation |
| `join`, `join-status`, `admission-inspect` | Directly targeted identified submission, retained status and original-issuer evidence; uncertainty has a bounded stop procedure |
| `cluster lock`, `cluster unlock`, `leave` | Identified authenticated mutations and exact receipt replay; local acceptance is not cluster-wide convergence |
| Removal/blocklist/tombstone administration, diagnose, workload/storage/log/audit commands | Not part of the supported worker formation surface; do not rely on imported commands or test-only fault hooks as production administration |

The verified operator path is source-built on Linux, using Unix sockets or
certificate-verified HTTPS/TCP. Peer transport is separate QUIC/mTLS. Secure
operator-token loading on non-Unix fails explicitly; other Unix platforms and
published release packages are not certified by the Linux process journeys.

From the repository root:

```sh
make build
make run-ctl ARGS="--help"
```

The client uses the local per-user worker socket by default. Select another
node with `--host` or `ORISHU_HOST`. Run `orishuctl <command> --help` for command
syntax. Imported command definitions do not prove worker support: currently
`cluster info` reads a real formation summary, and identified `cluster lock` /
`cluster unlock` requests update local replicated policy. Peer integration,
identified join/leave and recovery have passing process evidence; final task
acceptance remains tracked in the [formation task](../../docs/tasks/implement-cluster-formation-poc.md).

## Operator authentication

Local same-user summary reads need no credential. Remote summary reads require
the target worker's operator credential. On Unix, select its private token file
explicitly with `--operator-token-file` or `ORISHU_OPERATOR_TOKEN_FILE` (a path,
never the token value). An explicit flag overrides the environment setting.
The worker creates `operator.token` in its configured private state directory;
obtain a remote worker's token only through an existing secure administrator
channel. Do not use the formation join token or a monitoring credential.

```sh
orishuctl --host worker.example:8698 \
  --tls-cert /private/worker-server.pem \
  --operator-token-file /private/operator.token cluster info
```

Remote connections use HTTPS and validate server trust. `--tls-cert` adds a
trusted server certificate; it does not disable TLS validation. The token file
must be a regular, singly linked file owned by the current user with no group
or other permissions (normally mode `0600`). Its contents must match the
worker's format: exactly 64 lowercase hexadecimal bytes, without a newline.
Final-component symlinks, FIFOs, oversized and malformed files are rejected
before any request; errors never echo contents. Protect the containing
directory as well. Secure token-file loading on non-Unix platforms is not yet
supported and fails explicitly.

The option grants no additional authority by itself: the selected worker
validates the credential on each protected request. No automatic token search,
rotation or peer admission is implemented by this option.

## Identified membership lock changes

First inspect `cluster info`, then use its immutable formation ID (not its
display name). Both commands require the target worker's operator credential,
even over a local socket:

```sh
orishuctl --host /private/worker.sock --operator-token-file /private/state/operator.token \
  --output json cluster lock --formation-id FORMATION_ID --operation-id lock-001
orishuctl --host /private/worker.sock --operator-token-file /private/state/operator.token \
  --output json cluster unlock --formation-id FORMATION_ID --operation-id unlock-002
```

Choose a new operation ID for each new intent, unique among operators targeting
that worker/formation. After a timeout, retry the exact same command and ID
against the same worker. JSON/YAML/table output contains the original local
acceptance receipt, including operation, formation and source-node identities
and accepted policy version. It is not a claim that all peers have converged,
and replayed receipts are not fresh policy reads. Use `cluster info` to inspect
the current local state. A retry of `lock-001` after `unlock-002` does not lock
again; a new lock intent needs a new ID.

The PoC retains up to 1,024 outcomes per worker's formation lifetime and refuses
new IDs when full, preserving old retries. History is not durable audit and
expires with that formation participation; an old formation precondition then
fails. The response proves local acceptance, not a synchronous cluster-wide
fence; the three-worker harness separately polls convergence. Details and errors are in the
[client lock contract](../../docs/protocol-client.md#post-clusterlock).

Reproduce the CLI lock and three-worker introducer handoff after building both binaries:

```sh
cargo build --locked -p orishu-worker -p orishuctl
python3 scripts/check-formation-cli.py
```

`make test-formation` builds the binaries, tests evidence redaction and runs this
journey. On failure the harness prints a private temporary artifact directory
containing `report.json` and one log per worker. It retains at most 64 KiB of
each process's combined output and 16 recent non-token CLI results (4 KiB per
output stream); oversized CLI output is omitted. Raw logs stay in bounded
memory and are scrubbed before writing. Credential/key encodings and long opaque
values are deliberately redacted, including identity hex strings in raw output.
If credential parsing fails, raw logs and CLI output are withheld entirely.
No state directory, identity file, join-material file or command arguments are
archived. The artifact directory is `0700` and files are `0600`; review even
redacted diagnostics before sharing. Successful runs write no artifact bundle.

To test the harness's failure/cleanup path, run
`python3 scripts/check-formation-cli.py --inject-failure-after-startup`.
This intentionally exits nonzero after three-worker startup, preserves the
redacted bundle and reaps children. It is a harness-only test hook, not a
production worker fault endpoint or formation-protocol conformance test.

Run from the repository root on Unix. The harness tests authorization, lock,
unlock, retry, conflicting IDs, stale formation and shutdown with bounded
process cleanup. It starts three workers with duplicate labels: A admits B,
then B completes catch-up and admits C. It verifies assigned identities,
certificate bindings, exact replay, completed join status, live membership and
lock/unlock convergence through different workers. The runtime test separately
holds catch-up incomplete to verify withheld readiness/credential export, then
cancels a prepared attempt and verifies automatic recovery. The CLI happy path
does not establish interrupted-admission recovery or full fault conformance.
The harness also leaves C through the CLI, verifies fresh identities/token,
certificate retention, exact replay, stale/conflicting requests, standalone
no-op and eventual old-identity death in both surviving views. Its bounded
departure polling allows the default size-scaled SWIM suspicion period.
It then explicitly rejoins C using the retained certificate and checks a new
assigned node ID, retained dead history and replay of the old leave receipt
without another departure. The client library's `leave` method now requires a
typed `LeaveRequest` and returns `LeaveReceipt`; callers of the imported
parameterless leave method must migrate alongside the CLI flags.

Finally, the harness kills B with SIGKILL, observes SWIM suspicion and death,
then restarts it using the same state directory and client socket. It checks
fresh standalone identities/token, retained certificate/operator credential,
and absence of restored membership. Only an explicit join readmits B; every
view must then match the exact five-record history and three live identities.
This is ordinary crash/restart evidence, not permission to bypass a certificate
exclusion or a lost-admission-outcome recovery procedure. The test allows 60
seconds for crash detection and final reconciliation, covering size-scaled
suspicion and several five-second reconciliation rounds/ten-second round
timeouts; it does not promise a ten-second cluster convergence SLO.
Restart reuses one bounded log tail per logical worker. The harness refuses to
replace a still-running child or switch its state directory through that path.

See the [project architecture](../../docs/architecture.md) and root
[README](../../README.md).
