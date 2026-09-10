# orishu-worker

`orishu-worker` is the Orishu runtime daemon. A running instance is a node; a
cluster is a set of nodes cooperating to execute one workload and manage its
artifacts.

From the repository root:

```sh
make build
make run-worker
```

The worker now starts a fresh standalone formation and serves its real local
summary through `orishuctl cluster info`. Authenticated lock/unlock and an
explicit peer listener, automatic peer dialing and post-admission catch-up are
connected. The three-worker CLI handoff passes; recovery and lifecycle/fault
conformance remain unfinished.
Run `orishu-worker --help`
for the implemented interface. Deployment
examples live in `etc/`; the root `Dockerfile` produces a non-root worker image.

## Identity and local inspection

Peer listening is opt-in with `--listen.peers 127.0.0.1:6655` or
`ORISHU_LISTEN_PEERS`. The current PoC supports one literal IP/socket address.
`--advertise.peers` / `ORISHU_ADVERTISE_PEERS` overrides its advertised address;
wildcard binds require this override, which cannot be wildcard, multicast or
port zero. Without an override, a concrete bind on port zero advertises the
actual allocated port. YAML uses `spec.listen.peers` and `spec.advertise.peers`
as zero/one-element lists; precedence is file < environment < CLI.
Peer mTLS uses the private persisted worker identity, independently of client
TLS settings. The peer ALPN is now `orishu-membership/5`; older PoC profiles
cannot connect and must be rebuilt/restarted together. This adds source-side
bounded optional trace context while retaining admission attempt identity/replay
and unchanged membership Merkle hashes. All feature builds use profile 5;
there is no profile-4 fallback. See the [propagation evidence](../../docs/tasks/cluster-formation-conformance.md#profile-5-activation-and-cross-worker-receipt--2026-09-09).
Introduction is separately opt-in with `--accepts.peers true`,
`ORISHU_ACCEPTS_PEERS=true` or YAML `spec.accepts.peers: true`; the default is
false, and enabling it without a peer listener fails startup. Explicit CLI
false overrides an enabled environment/file setting. The startup role advertises
64 peer connection slots; the registry bounds live sessions to 64, independently
of the core's local admission-view limit. This is not a globally reserved member
capacity. Adopted workers remain `catchingUp`, non-introducing and without target
credential export until automatic catch-up validates and installs the complete
admission baseline and target credential. The scheduler permits one active job,
with at most three prepared attempts within 90 seconds of adoption. It checks
for eligible work once per second; absent routes do not consume attempts.
After adoption it prioritizes one fresh admitted handshake to the original
introducer before the normal peer scan; this is not a new admission or retained
bootstrap session. Peer maintenance uses up to four candidates per tick within
the existing four shared dial slots and one-second owner-query budget.
Adoption wakes the first scan, and registration of the first current admitted
route wakes initial catch-up. Failed attempts retain periodic retry scheduling;
these wake-ups do not reset any budget or grant admission/readiness.
Success publishes `joined`; failure remains in the target formation with
`catchUpFailed` operation status and no introduction readiness. Lost admission
ACKs can recover the original still-live assignment through the same pinned
introducer. Retirement, exclusion or unavailable replay state still leave an
uncertain outcome; do not blindly start another join. A bounded
pending-join redial loop now retries a lost connection to the originally pinned
introducer while the same operator attempt is pending. It permits one job and
at most eight preparations, retaining the core retry budget; it does not choose
a new target. Each introducer retains at most 1,024 accepted attempts for its
local formation lifetime, without eviction; new attempts refuse at capacity,
but exact replay remains available. This is not durable or cluster-wide replay.
Join status uses resource schema version 2 and retains a secret-free
`recoveryReference`: peer attempt ID, applicant fingerprint and original
introducer ID/fingerprint. It survives discarded transient join IO but not
process restart. An explicit null is not proof that remote admission never
occurred. Rebuild operator clients with the worker for this resource change;
the reference is diagnostic correlation, not permission to retry or a complete
recovery procedure.
The operator-authenticated `POST /api/v1/membership/admission-inspections`
queries the original issuer's retained attempt/assignment evidence. It is
read-only, bounded to 4 KiB and five seconds, and shares the 16-request
inspection budget. Wrong issuer or absent history is an unknown outcome, not
proof of refusal. See the [CLI manual](../orishu-ctl/README.md) for the exact
command and interpretation. This does not enable new admission or bypass a
current restriction.
Follow the [admission recovery runbook](../../docs/cluster-admission-recovery.md)
for bounded polling and mandatory stop conditions after uncertainty or restart.
Jobs contain no join
token and are cancelled on lifecycle change/shutdown. Separately, a bounded
maintenance loop now reconnects admitted peers automatically: the lower assigned
node ID dials when both advertise endpoints, while an unadvertised worker dials
the reachable peer. The PoC assumes stable advertisements and all-to-all peer
reachability; reconnecting never enables introduction or completes catch-up.
No peer port opens
without explicit configuration. Listener exit supervises the membership owner,
and owner shutdown closes established and pending peer connections.

For the intermittent post-leave readmission convergence regression, the
diagnostic subset below runs the same public three-worker handoff, lock/leave,
receipt replay and readmission assertions, then performs normal bounded
shutdown before the later crash/restart stages:

```sh
cargo build --locked -p orishu-worker -p orishuctl
python3 scripts/check-formation-cli.py --readmission-only
```

It also omits the separate standalone CLI checks and cannot combine with other
scenario flags. It uses the same assertions/budget, adds no worker fault controls
and does not
replace `make test-formation` for acceptance. See the
[conformance ledger](../../docs/tasks/cluster-formation-conformance.md#current-process-regression-requiring-resolution)
for failures and reproduction status; a passing diagnostic run is not a fix.
After the existing four-record/three-live summary check, every worker's public
membership list must contain the exact expected IDs, certificate bindings and
liveness states, including the departed record. This check uses the remaining
shared polling budget, not a fresh window; duplicate labels and
matching counts cannot substitute for matching identities.
The readmission polling budget is seventeen seconds: the current ten-second
reconciliation round timeout, up to five seconds until the next owner cadence,
and two seconds of scheduling margin. A controlled real-wire fixture retained
three live original identities before departure and demonstrated that a round
against the departed peer can delay learning the new identity for about fourteen
seconds. The former ten-second cutoff was insufficient for that supported
ordering. This is not a runtime timeout change or a production convergence SLO;
the historical process failures lack internal timing evidence to establish
their precise cause. See the conformance ledger for the limits of this finding.
If the post-leave convergence assertion fails, the harness makes one additional
public membership-list request per worker. Its failure report correlates fixed
roles (original/second introducer, departed and readmitted member) with record
counts, finite liveness states and certificate-binding matches, without
exporting identity strings. Missing records and unavailable reads are distinct.
These post-failure observations are not an atomic snapshot or a retry of the
assertion; failure capture permits three additional three-second subprocess
waits before cleanup. The original failure remains a failure even if views
converge later.

Only `--readmission-only` adds a second diagnostic snapshot: after the initial
capture, it waits twenty seconds and makes one more public `ls` request per
worker. The report keeps both `readmission-diagnostic` and
`readmission-late-diagnostic` projections. This mode permits at most six
three-second subprocess waits plus the twenty-second observation delay
(38 seconds plus bounded local capture work) before failure cleanup. Later
convergence never changes the failed assertion or command exit to success.
Normal and fault conformance journeys retain the single immediate capture.

Harness-only `make test-formation-policy-partition` instead uses ordinary
binaries and bounded loopback UDP relays to isolate encrypted peer links while
Unix operator endpoints remain available. It checks divergent lock/unlock
views, exact policy-version convergence after healing, unchanged identities,
and the full leave/crash/restart journey. The short fault window does not prove
recovery after long partitions have retired a member. Relays are test tools,
not a supported production proxy or worker fault-control API.

`make test-formation-client-pressure` exercises slow authenticated request bodies
through the real Unix client listener while three workers exchange formation
traffic. It fills the 16 mutation-handler slots, requires prompt overload
refusal, checks independent status reads and peer lock convergence, then frees
one slot and requires an authenticated local unlock before body deadlines can
expire. The ordinary handoff/leave/crash/restart journey surrounds this case.
At final shutdown it retains an admitted incomplete body and another socket
with unfinished headers until every worker exits, requiring a shared
three-second deadline and client-socket cleanup. The owner supervisor alone
initiates client shutdown, with a one-second drain after owner termination.
The same journey holds sixty accepted connections at incomplete headers while
peer policy and legitimate status reads progress, before header expiry can
mask the pressure. Separate real-worker tests fill all 64 connection slots,
verify an established status connection still works, reclaim a slot and test
header expiry. Each listener admits at most 64 connections; additional clients
wait in the OS backlog. Client TLS has an explicit ten-second handshake fallback;
the current server's five-second initial protocol-detection deadline wraps the
lazy handshake and therefore closes silent/trickled unfinished TLS earlier.
Authenticated control on other connections remains available below capacity.
These deadlines do not promise service against sustained listener saturation.
HTTP/1 heads have a five-second deadline, 8 KiB buffer
and 32-header limit; oversized/excessive heads receive HTTP 431. These fixed
PoC limits apply before authentication and do not replace handler budgets.
A requested transport write stalled for five seconds closes the connection;
this is not a whole-response timeout for a client making progress. A focused
real-Unix-listener test uses the production summary and lock handlers to prove
pending writes, stopped pipeline processing, independent authenticated control
and server-side reclamation while the slow client remains unread. It does not
claim a minimum throughput or test future streaming routes. A lost mutation
receipt still requires the documented outcome/replay procedure.

The worker integration suite also checks all supported mutation intents with
missing, wrong, foreign-worker, join and duplicate/mixed operator credentials
over Unix and certificate-verified TLS using both HTTP/1.1 and HTTP/2. These tests require
actual authorization rejections and unchanged state, followed by authorized
control. The HTTP/2 tests explicitly negotiate `h2` on TLS and additionally
verify authorized lock/unlock/leave and identified join submission over raw
HTTP/2. They do not infer completed catch-up from submission. HTTP/2
flow-control evidence is mapped in the
[accepted formation ledger](../../docs/tasks/cluster-formation-conformance.md#ingress-pressure-case-to-contract-closure--2026-09-07).
HTTP/2 has
explicit per-connection limits: 16 streams, 8 KiB decoded headers, 4 KiB HPACK
table, 16 KiB frames, fixed 65,535-byte initial receive windows and a 16 KiB
per-stream send-buffer setting. A real Unix HTTP/2 test verifies stream refusal,
cancellation/reuse, oversized-header rejection and shutdown with unfinished
streams. Further real-process Unix tests exhaust stream and connection response
credit, verify DATA stops, and restore credit to complete the blocked responses.
Stream cancellation frees capacity; independent authenticated control and
shutdown work while other responses retain zero credit. This does not prove a
deadline for withheld credit, inbound flow-control violation handling or
incomplete HTTP/2 headers; socket write timeouts do not prove those bounds.
Separate Unix wire/server tests now verify exact inbound receive-credit limits,
one-byte overruns and invalid WINDOW_UPDATE refusal. The stream-only overrun
fixture increases connection credit solely to isolate the unchanged stream
limit; deployed worker limits remain as stated above. Legal update controls
and a successful request after stream reset prevent a blanket-disconnect
implementation from passing. Zero window increments can close the entire
connection even when addressed to one stream; clients must recover uncertain
operations through the normal status/replay procedure. These decoder tests
do not establish a deadline for withheld response credit.
The worker now separately enforces a five-second absolute HTTP/2 response
delivery budget from its first outgoing HEADERS byte until complete END_STREAM
delivery to the plaintext transport (including final header continuations).
Waiting for stream or connection credit, partial DATA progress and PINGs do not
renew it. A stream reset cancels its response budget. Expiry closes the entire
offending connection; other connections and membership continue independently.
Partially written outgoing frames have the same five-second absolute budget.
This is not remote receipt acknowledgement or a maximum connection age. A
missing mutation receipt still requires the normal operation-status/replay
procedure, never an assumption of rollback. Real Unix/TLS expiry tests and a
reset/completion control exercise this boundary.
The continuation regression separately checks the pinned decoder's five
non-final-frame budget and same-stream requirement, with real GOAWAY errors
and a valid completed-header control. Frame-count rejection is not
elapsed-time reclamation: a separate five-second absolute assembly deadline
now bounds each incoming HTTP/2 frame and each whole header block, including
its CONTINUATION frames, from the first byte. A partial connection preface has
the same budget. Trickle progress does not reset these deadlines. Expiry closes
the entire offending connection, not just a stream; clients must reconnect and
use operation-status/replay rules for any uncertain mutation outcome. This is
not a connection-age cap; response delivery uses its separate budget above.
The observer runs on plaintext for both Unix and TLS client listeners and
does not change peer membership. See the [client ingress contract](../../docs/protocol-client.md#formation-client-ingress-limits).
The client listener now closes Unix/TLS HTTP/1.1 and HTTP/2 connections after
ten seconds without a successful transport read or write. Idle clients should
reconnect; this does not roll back an accepted operation. A real-worker HTTP/2
test retains an unfinished header block, a partial frame payload and a response
with zero window credit until the server closes each connection. Active peers
that trickle bytes or use other streams can avoid connection idleness; this
does not establish a per-stream response deadline. The shared watchdog
independently enforces input assembly and response delivery even when other
traffic keeps the connection active.

The internal owner health projection is now supervision-backed: its actual
control-loop tick publishes progress once per second, and five seconds without
progress marks it stalled. Polling cannot refresh that signal. The optional
probe listener below consumes this projection. Process health latches startup after configured
listeners bind and withholds readiness on required-listener exit or shutdown;
later failures never reset startup success. Secured remote diagnostics, broader
metrics and trace exporters remain planned beyond the local surface below. See the
[observability design](../../docs/orishu-observability.md#implemented-owner-supervision-input).

### Optional local diagnostics

The initial capability serves `/metrics`, `/livez`, `/readyz` and `/startupz`
on a separate loopback listener. Enable it explicitly:

```sh
cargo run --locked -p orishu-worker --features observability -- --observability.enabled true
curl --fail http://127.0.0.1:9168/readyz
curl --fail http://127.0.0.1:9168/metrics
```

Both route groups are enabled by default once the listener is explicitly
enabled. For probes without metrics add `--observability.metrics false`; for
metrics without probes add `--observability.probes false`. Disabled routes
return `404`. Selecting routes alone never enables diagnostics, and enabling
the listener with both groups disabled is a configuration error. Probe-only
mode remains loopback-only; it is not an unauthenticated remote probe exemption.
Use exact GET paths with no query parameters. Other methods (including HEAD)
on enabled query-free paths return 405 with `Allow: GET`; unknown/disabled paths
return 404, and queries on enabled paths return 400 before method checking.
Dispatched errors are non-cacheable fixed plain text; HEAD has no body. Encoded,
case or trailing-slash aliases are not supported. These transport/route errors
are not the worker's health 503 and do not authorize restart or readmission.
File and environment equivalents follow the same precedence as listener
enablement; see [configuration](../../docs/orishu-configuration.md#worker-configuration).

`--observability.bind` selects another loopback address/port. The build feature
and runtime listener are both off by default; requesting enablement from a
build without the feature fails startup. Non-loopback binds are rejected.
An occupied diagnostics port produces a clear startup error (exit code 2)
before worker credentials or client sockets are created. The worker never
removes or replaces the conflicting listener; select a free port or resolve
the owning service's configuration.
Diagnostics share a sixteen-connection budget and the bounded HTTP server's
five-second stalled-write timeout. A full diagnostics budget can make probes
unreachable while operator/owner work remains healthy; do not equate that
transport failure with peer death. The [slow-reader tests](../../docs/tasks/cluster-formation-conformance.md#diagnostics-response-backpressure--2026-09-07)
establish real HTTP/Unix backpressure and slot recovery, not TCP/proxy or
whole-process shutdown-under-write-pressure qualification.
The current metrics comprise three health gauges and six process-lifetime
owner counters for transitions, stale inputs, core diagnostics, foreign gossip,
reliable replies and failed sends. Three further counters distinguish local
admission insertion, core refusal and validated retained-assignment replay.
Replay is not another insertion, and pre-core failures are not core refusals.
Counts survive formation changes but reset on process restart; transport and
request counts are not accepted-command counts. See the
[catalogue](../../docs/orishu-observability.md#implemented-local-surface).
This is not full formation/transport instrumentation.
Eight additional gauges report occupied slots and capacity for the peer,
control, completion and shutdown lanes. Occupied slots include reserved
completion permits, not only queued messages. Saturation does not itself
establish worker death, and a scrape cannot consume owner queue capacity.
Six client-service instruments add in-flight, completed, HTTP 4xx/5xx and
cancelled request counts plus cumulative completed-handler duration in seconds.
They retain no request data or labels and are active only when runtime metrics
are enabled. Handler completion is not network delivery or domain acceptance;
cancelled futures are separate from completed responses, and early transport
rejections are excluded. A separate owner counter reports membership-packet
session-binding/decoding refusals, excluding transport framing, handshakes and
catch-up. Three receive counters report SWIM packets, anti-entropy packets and
gossip items (envelope plus pull-reply deltas). Repeated/ignored traffic counts;
these are not successful probes, newly merged records or convergence evidence.
An aggregate completed-handler duration histogram adds fixed buckets from 1 ms
through 5 s and `+Inf`; cancellation and socket delivery remain excluded.
Scrapes never count themselves. Ten additional inbound peer counters distinguish
TLS/application handshake completion, failure, timeout, cancellation and the
two connection-capacity refusals. They do not count accepted admissions or
establish peer death. Collection follows metrics enablement, independently of
tracing, and has no counter allocation in probes-only/disabled modes. See the
[inbound catalogue](../../docs/orishu-observability.md#inbound-peer-handshake-counters).
Four [inbound capacity gauges](../../docs/orishu-observability.md#inbound-connection-capacity-gauges)
also expose the actual TLS permits and connection-task occupancy/limits.
Completed tasks awaiting collection still occupy a slot; these are not counts
of established sessions or admitted members. With collection enabled but no
peer adapter running, occupancy and capacity are zero. Adapter termination
withdraws the gauges while retaining cumulative event counts.
The shared reliable-exchange pool additionally measures request/serve outcomes,
duration, partial stream bytes and slot occupancy/capacity. Bytes are published
at exchange termination; locally accepted writes do not prove remote receipt.
See the [reliable-exchange catalogue](../../docs/orishu-observability.md#reliable-peer-exchange-metrics)
for precise boundaries and safe interpretation. Eight additional
[traffic counters](../../docs/orishu-observability.md#datagram-and-pre-pool-traffic-counters)
report datagram submission outcomes/payload bytes, pre-validation reception and
the per-connection stream-task refusal gate. Submission is not delivery, and
these counters cannot establish all datagram loss. The
[outbound catalogue](../../docs/orishu-observability.md#outbound-dial-and-tls-metrics)
adds whole-attempt and individual TLS-candidate outcomes/durations plus four-slot
dial pressure. Candidate fallback and owner acceptance remain distinct: a
completed attempt has only returned a reply for validation. The
[membership deadline catalogue](../../docs/orishu-observability.md#membership-deadline-and-abandonment-counters)
adds consumed timers, stale timer input and join/reconciliation abandonment.
Cancellation is not expiry, and abandonment does not prove non-admission.
Four [registry gauges](../../docs/orishu-observability.md#registered-session-capacity-gauges)
separately expose retained authenticated-session usage/capacity and the shared
provisional subset, including outgoing introducer bindings. Closed entries
remain counted until normal pruning; registration is not admission or proof
of a live socket. An active empty registry reports capacities 64/16 even without
a peer listener; owner drop withdraws all four values to zero. Inspect operation
status and related pressure/refusal signals before acting, never restart or
readmit solely because these gauges are high.
The [catch-up counters](../../docs/orishu-observability.md#admission-state-catch-up-outcomes)
separate whole-job receiver outcomes from the owner's adoption, non-adoption,
fencing and abandonment decisions. Pages are not separate attempts; a validated
transfer can still be fenced after a lifecycle change. Inspect authenticated
join-operation status before taking action, never infer successful admission or
authorize restart/readmission from these counters.
The base 151-series exposition
requires a 32 KiB scrape budget (previously 50 series within 16 KiB). Per-route,
inbound-handshake and broader formation-stage latency remain separate work.
Validate the current catalogue and a real local scrape with
`make test-worker-prometheus`; the [test guide](../../docs/testing-worker-prometheus.md)
documents pinned tools and the runnable local scrape configuration.
Local client-service tracing is available with `--features otlp-tracing`.
Enable it with `--tracing.enabled true` and an explicit `--tracing.endpoint`.
HTTP is limited to literal loopback; HTTPS uses `--tracing.ca-file` or the
documented bounded Linux system bundle. Other trust layouts need an explicit
CA file. The worker consumes the
`--tracing.*` sampling/queue/batch/timeout settings documented in the
[configuration guide](../../docs/orishu-configuration.md#implemented-tracing-budget-configuration-exporter-unavailable),
including bounded request/response bytes and shutdown drain. Tracing is disabled
by default and independent of metrics. Invalid credentials or an omitted build
capability fail startup; disabled tracing makes no collector connection.
With metrics and tracing both enabled, twelve unlabelled trace delivery/loss
counters extend the catalogue to 163 series within the same 32 KiB scrape budget.
They remain readable during collector stalls without waiting for export and
are absent when tracing is disabled or omitted. See the
[catalogue and safe troubleshooting](../../docs/orishu-observability.md#live-trace-delivery-and-loss-counters).
`make test-worker-trace-prometheus` validates all 163 series through a real
Prometheus server, including fresh delivery counts after collector recovery;
see the [test guide](../../docs/testing-worker-prometheus.md#ingest-trace-counters-through-prometheus)
for pinned tool prerequisites and the supported local source-build scope.
Profile-5 admission propagation and [structured local span-correlated logs](#structured-stdout-logs)
are implemented with scoped process evidence; full M4 acceptance remains open.
`make test-worker-otelcol OTELCOL=/absolute/path/to/otelcol` runs the
[pinned local Collector walkthrough](../../docs/testing-worker-otelcol.md):
real OTLP decoding/file receipt, disabled/zero sampling and collector
shutdown/recovery while authenticated control remains usable. The guide also
shows how to inspect a retained local span. This is a loopback diagnostic
recipe, not a remote collector or durable tracing backend.
`make test-worker-otelcol-mtls` exercises the corresponding
[mTLS receiver recipe](../../docs/testing-worker-otelcol-mtls.md), using
collector-only trust/client keys and testing certificate/name refusals and
control continuity. Never reuse peer/operator/monitoring credentials for export.
`make test-formation-observability` checks issuer-local counts through HTTP
during the full public three-worker churn journey. The separate
`make test-formation-observability-lost-ack` target requires a debug fault build
and checks positive assignment-replay counts without duplicate insertion.
Run these builds sequentially; see the same guide for scope and limits.
`make test-formation-observability-ejection` checks real HTTP probes after
authenticated wire ejection and explicit leave: the excluded process stays
live but unready, startup stays latched, and peer loss does not fail survivor
health. It is a separate debug-fault scenario, not the full churn journey.
`make test-formation-observability-issuer-loss` checks live-but-unready probes
through actual admission retry exhaustion after issuer loss. It preserves the
operator stop procedure; startup success does not resolve uncertain admission.
Formation harnesses now retain private executable copies for the entire run,
so later feature builds cannot silently change restart behavior.
`make test-formation-release-guard` checks the normal release profile, then
requires the precise compile-time refusal when `formation-fault-test` is
requested. CI runs this gate; an unrelated build failure is not accepted as
proof of exclusion. Ordinary executable tests also reject every fault flag
before creating state or a client socket. This checks the repository's default
release configuration, not published artifacts or custom profile overrides.
The [mTLS monitoring proxy recipe](../../docs/testing-worker-monitoring-proxy.md)
now tests ADR 0017's secure-proxy option with real Nginx and Prometheus while
the worker remains loopback-only. Its
[stalled-reader journey](../../docs/testing-worker-monitoring-proxy.md#downstream-response-backpressure-and-expiry)
checks actual TLS write pressure, bounded generation, timeout-driven request
slot reuse and independent operator progress; it is not a fleet-scale or
arbitrary slow-client guarantee. Worker-native remote diagnostics, sampled
Cross-peer OTLP traces, broader metrics and service/container/release handoff
remain pending.
See the [configuration mapping](../../docs/orishu-configuration.md#worker-configuration).

Reserved join/reconnect/catch-up completions retain explicit disposal ownership:
late results after owner shutdown must release their connections and credential
state even while callers retain worker handles. A real pinned-handshake
regression verifies connection closure after leave and shutdown; it does not
make a cancelled or stale handshake an accepted admission.

Development-only process fault conformance uses `make test-formation-lost-ack`.
It builds into `target/formation-faults` with the explicitly selected
`formation-fault-test` feature, which is absent by default and rejected when
debug assertions are disabled (including ordinary release builds). Never ship
that build as an operator package. The startup-only
`--test-lose-next-join-ack` switch exists only in that build and requires
loopback peer binding, Unix client listeners and a private state directory.
There is no remote fault-control endpoint or environment-variable switch.
The one-shot hook discards acceptance after insertion and ledger commit,
closes sessions, and reports the original secret-free assigned ID to the
bounded harness log. The harness must match it to the recovered public status;
ordinary successful formation is not enough to pass this fault case.

`make test-formation-issuer-loss` selects the separate
`--test-crash-after-join` startup fault under the same build/exposure guards.
It abruptly exits the introducer with code 86 after insertion and ledger commit,
before returning acceptance. The harness waits for the source's actual default
retry exhaustion (183 seconds of timer windows, with a 210-second test cutoff),
checks retained unresolved status and refusal of new joins/leave, then restarts
only the dead introducer to verify `wrongIssuer`. Up to that point neither the
source's retry budget nor its identities are reset. A subsequent test-injected
source crash checks that retained credentials do not restore operation history
or the old assignment: status must report `UnknownOperation`, the worker starts
standalone with fresh identities, and no new join is submitted. This second
fault is not a recovery instruction; missing history still requires operator
stop/review. The startup fault switches are mutually
exclusive, have no environment aliases, and remain unavailable in normal builds.

`make test-formation-source-loss` uses the lost-ACK fault with a surviving
introducer. On the confirmed insertion marker, the Unix harness briefly sends
`SIGSTOP` to the issuer, verifies the source is still admitting, kills and
restarts only the source, then resumes the same issuer with `SIGCONT`.
The issuer must retain the original assigned ID; the restarted source must
report `UnknownOperation` and fresh standalone identities despite retained
credentials. A missed pre-adoption window fails the test. No join is submitted
after restart, and failure cleanup kills/reaps paused processes too. Pausing
or restarting workers here is test fault injection, not a recovery recipe.

`make test-formation-dead-assignment` instead preserves both original
processes. After confirmed insertion and ACK loss, it pauses the source until
the issuer's normal SWIM timers mark the assignment Dead (60-second observation
cutoff), then resumes the source's existing bounded retry path (210-second
cutoff). Public inspection must keep the old assignment Dead and report
`retiredOrRestricted`, without a replacement member. The source must finish
unresolved with its original identities/reference; exact request replay cannot
reset the attempt, and a new join or leave remains refused. This is a Unix
fault-harness mode using the same development-only hook, not an administrative
removal API or a recommendation to pause a worker during recovery.

`make test-formation-removed-assignment` selects the development-only
`--test-remove-after-join` startup fault. After insertion and ledger commit it
discards the acceptance and applies the existing force-removal command through
the serialized membership owner, then closes sessions. The marker identifies
the removed assignment only after that transition. The public harness requires
`retiredOrRestricted` inspection, no old or replacement member in the issuer's
list, and bounded exhaustion of the original source attempt without changing
its identities or reference. Exact request replay cannot reset exhaustion;
new join and leave requests refuse. The same loopback/Unix/private-directory
guards apply, and the switch conflicts with the ACK-loss and issuer-crash
switches. It is not a new public removal API or a normal-build capability.

`make test-formation-blocked-assignment` selects the development-only
`--test-block-after-join` fault under the same guards. It applies the existing
blocklist transition to the authenticated applicant certificate fingerprint
after insertion and before acceptance delivery. The member record is retained;
public inspection must report `retiredOrRestricted` throughout recovery, with
the same fingerprint and no replacement member. The original source must
exhaust its unchanged retry budget and refuse new join/leave work. No public
blocklist API, certificate rotation or exclusion-clearing action is added.
This case does not test ejection of a worker that has already adopted its identity.

`make test-formation-excluded-restart` covers the separate negative admission
case: restart the blocked source with its private files retained, verify fresh
standalone identities and missing old operation history, then deliberately
submit a fresh attempt against the original issuer. The certificate must remain
blocked: the new attempt cannot adopt or allocate a replacement and ends
unresolved within the existing 210-second observation cutoff. The issuer still
reports the original assignment restricted and no accepted record for the new
attempt. A third worker with a different certificate must join successfully
through that same issuer and material. This test deliberately attempts a
forbidden bypass; it is not an operator recovery recipe or permission to submit
a fresh attempt after history loss. No additional worker fault switch is used.

`make test-formation-peer-ejection` tests an already-joined worker learning its
own removal through authenticated peer gossip. Its development-only
`--test-eject-peer-on-signal` startup fixture registers one Unix `SIGUSR1`
trigger before client listeners open. Once the harness verifies B is joined,
it signals A, which sends one bounded tombstone-bearing datagram over the
current authenticated session to its sole live peer. A joined test worker can
instead target its recorded introducer by immutable node identity, allowing a
separate unresolved admission to coexist in the fixture. The fixture refuses an
absent/ambiguous peer or a tombstone that does not fit the datagram; it never
creates an offline queue or retries. The same startup exposure guards and
mutual exclusions apply; no normal-build signal handler or remote control API
is added.

`make test-formation-issuer-ejection` exercises this second case: A inserts B
but loses its ACK; the harness pauses B, admits C normally, and signals C to
send A's tombstone over its authenticated connection. A becomes ejected with
unchanged identity but clears its accepted-assignment history. The same issuer
therefore reports `recordUnavailable`, not `wrongIssuer`. B resumes its original
bounded attempt and must stop unresolved without adopting or allocating a new
identity. The harness checks exact replay and refusal of fresh join/leave.
These are development fault controls, not an operator recovery recipe.

This fixture acts as a test peer: it does not apply removal to A's model and
does not prove cluster-wide removal convergence. B must report `ejected` with
its old identifiers, stop introduction/live peer participation, retain its
historical join receipt, and remain ejected until an explicit lifecycle action.
The harness then verifies supported explicit leave to fresh standalone state.
This is separate from the excluded-restart test and is not an operator removal
command. The HTTP rejection checks also require a real bounded response when
a server closes the request side before its body finishes sending; a broken
pipe by itself never counts as successful rejection.

`make test-formation-lost-departure` runs the full three-worker journey with
`--test-drop-next-departure` armed on the departing worker. This development-
only, one-shot startup fault suppresses the next leave transition's encoded
departure datagrams at the existing send boundary, without changing the core
transition, receipt, identity change or session fencing. The harness requires
the old assigned-ID marker and a count of exactly two suppressed announcements.
Survivors must still mark the departed ID Dead through SWIM within the existing
60-second observation cutoff. The same certificate then rejoins with a fresh
assigned ID while old dead history remains, without clearing exclusions.
The rest of the full crash/restart/readmission journey and bounded cleanup
remain enabled. All existing fault-build exposure guards and mutual exclusions
apply; no normal release flag, runtime reconfiguration or remote fault API is
introduced.

`make test-formation-lost-leave-response` uses ordinary worker builds and a
one-request Unix-socket proxy in the harness's private directory. The proxy
forwards the CLI's authenticated request unchanged, verifies HTTP success from
the worker, and closes without sending any response bytes to the CLI. The
test then verifies the worker already became standalone and retries the exact
operation through the normal endpoint. The recovered receipt must identify
that same transition, without a second identity change. It continues through
readmission and the full churn journey, including historical receipt replay
after readmission. The proxy bounds request headers/body and IO time, retains
no raw requests/responses in artifacts and adds no worker fault flag or API.

A worker that learns its assigned identity was removed reports `ejected` in
local `cluster info`. It retains the formation/node identifiers for diagnosis,
but closes peer sessions, drops its formation credential and stops membership
participation. It does not silently create a new formation or rejoin. Local
inspection and shutdown remain available. Restart creates fresh standalone
formation/node IDs while retaining the certificate, so restart is not a way to
bypass certificate-based exclusion. Authenticated identified `orishuctl leave`
can also return a joined, catching-up or ejected worker to standalone. Public
leave has three-worker CLI evidence; interrupted-admission recovery and full
certificate-exclusion/ejection conformance remain unfinished. The CLI harness
also verifies ordinary SIGKILL/restart: survivor suspicion/death, fresh
standalone IDs, retained private identity/operator credential, and explicit
readmission with a new assigned ID. No restart automatically rejoins the old
formation. See the
[leave instructions](../orishu-ctl/README.md) for retry/receipt semantics.

Each process generates fresh formation/node IDs. Its certificate and local
operator credential persist in a private directory selected by `--state-dir`,
`ORISHU_STATE_DIR`, or `spec.stateDir` in the YAML configuration, in that
precedence order. The default is `$XDG_STATE_HOME/orishu/worker`, falling back
to `$HOME/.local/state/orishu/worker`. Paths must be absolute. Only one worker
may own that directory; multiple instances need distinct state directories
and client sockets. Secure credential persistence currently supports Unix.

Worker labels use `--name`, `ORISHU_WORKER_NAME`, or YAML `name` (default
`orishu-worker`). Standalone formation labels use `--cluster-name`,
`ORISHU_CLUSTER_NAME`, or YAML `cluster.name` (default `standalone`). Labels
need not be unique and never identify a formation or member.

Local sockets are created owner-only (`0600`), and reads require no token.
TCP listeners require a configured TLS
certificate/key and the private `operator.token` bearer credential for reads.
The CLI accepts `--operator-token-file` (or the path environment variable
`ORISHU_OPERATOR_TOKEN_FILE`) for authenticated requests; see its
[authentication instructions](../orishu-ctl/README.md#operator-authentication).
Never put this token in workload files or logs.
`identity.json` contains private key material; neither file may be made
group/world-readable. Corrupt files are startup errors, not permission to
silently create a different identity. See the
[credential contract](../../docs/protocol-p2p.md#local-credential-storage-formation-poc).

```sh
orishu-worker --state-dir /absolute/private/worker-a --listen.clients /absolute/private/a.sock
orishuctl --host /absolute/private/a.sock --output json cluster info
```

The response explicitly reports its source node, formation ID, local freshness,
no workload and whether introduction is ready. Introduction requires explicit
role configuration and validated local admission readiness. Membership is owned
by one task, and a stopped owner makes summary reads unavailable and shuts down
the client listeners. Graceful shutdown removes the owned Unix socket. Restart
can recover an owned stale socket after a refused connection, but never deletes
an active listener, symlink or unrelated file. The socket's immediate parent
must belong to the current user and must not be group/world-writable; use a
private directory rather than placing the socket directly in `/tmp`.

The worker also serves identified `POST /api/v1/cluster/lock` requests through
the shared client and `orishuctl cluster lock` / `cluster unlock`. Every mutation
requires the worker-local operator token, including over Unix sockets. Request
formation preconditions are checked by the owner; exact retries recover the
original local acceptance receipt without reapplying old intent. The receipt
alone does not demonstrate peer convergence; the three-worker harness checks
that separately. See the
[CLI workflow](../orishu-ctl/README.md#identified-membership-lock-changes) for
operation IDs, the explicit 1,024-outcome formation-lifetime limit and testing.

### Structured stdout logs

Unix source builds support `--logging.enabled true`, independently of both
telemetry features. Logging defaults to disabled. `--logging.queue-records`
defaults to 256 (1–4096), and `--logging.shutdown-ms` to 250 (0–2000 ms).
The [configuration table](../../docs/orishu-configuration.md#implemented-structured-stdout-logging)
lists file/environment equivalents and precedence.

When enabled, stdout contains only the fixed version-1 JSON event catalogue
during ordinary successful runtime operation. Startup errors remain separate on
stderr. Existing synchronous listener/signal/final-export-statistics prints are
removed; disabled logging does not retain them. Development fault markers and
panic diagnostics are not this production logging interface.

Lifecycle records are unsampled. Client/admission/peer-operation records follow
actual local trace sampling; zero sampling or disabled tracing emits none of
those records. To correlate, match a received OTLP span's trace and span IDs to
the record's `trace_id` and `span_id`; `unix_nanos` is its completion timestamp.
Do not treat a log as command acceptance or a guarantee of collector delivery.
No names, raw paths, credentials or arbitrary error fields are emitted.

If metrics are also enabled, inspect the nine `orishu_worker_log_*_total`
counters. Rising `queue_full` or `contended` means records were shed; check the
external stdout reader. Rising `output_failed` followed by `closed` indicates
terminal sink failure, not failed membership. Do not restart or alter formation
solely because telemetry is missing. Restore a slow reader where possible;
replacement of a closed output requires a separately planned worker restart.
Keep per-process collection metadata outside the log record.

The [logging-counter Prometheus check](../../docs/testing-worker-prometheus.md#ingest-logging-counters-through-prometheus)
verifies actual backend ingestion with tracing disabled or enabled, and with a
real broken stdout pipe. Logging failure leaves control and probes healthy;
after scraper recovery, new writes/closure refusals must appear in Prometheus.

After resuming a stalled stdout reader, check that acknowledged `written` counts
advance, existing loss counts remain visible, and a new operation yields a usable
record. A quiet, drained queue can have `written == accepted`; while producers
are active these independent counters are not a transactional equality check.
Already-shed records are not replayed. The
[real-worker recovery test](../../docs/tasks/cluster-formation-conformance.md#real-worker-stdout-reader-recovery--2026-09-09)
verifies resumed output and a fresh authenticated trace ID without restarting or
changing formation. This does not recover a terminally closed/broken output.

Shutdown drains only for its configured interval and may leave one write's
delivery uncertain. There is no fallback stderr report, retry destination or
durable audit guarantee. The last metrics scrape is not final shutdown
accounting. With tracing enabled, twelve final `orishu.trace.accounting` records
carry fixed `counter`/`value` pairs through the same bounded queue. They may be
shed or only partially delivered; they describe trace export, not final logging
loss, and do not revive the removed stderr summaries.
See the [record/loss contract](../../docs/orishu-observability.md#bounded-operational-log-adapter)
and [scoped executable evidence](../../docs/tasks/cluster-formation-conformance.md#runtime-logging-and-local-span-receipt--2026-09-09).
The [official Collector three-worker walkthrough](../../docs/testing-worker-otelcol.md#official-collector-formation-and-log-walkthrough)
now matches both admission chains to their workers' stdout records alongside
direct scrapes, probes and cross-worker policy checks. The separate
[systemd collection check](../../docs/testing-worker-user-service.md#verify-enabled-journal-and-trace-collection)
and [rootless-container collection check](../../docs/testing-worker-container.md#verify-enabled-container-and-trace-collection)
now match actual journal/runtime records to received spans across deliberate
restarts. These are local collection checks, not full supervisor-specific
formation or performance qualification; the final M4 checklist remains open.

## Monitoring interface and remaining work

The [source-built user-service recipe](../../docs/testing-worker-user-service.md)
provides tested runtime-only systemd start/stop, feature-disabled and probes-only
modes, private credentials and explicit fresh-identity restart. It has no
automatic restart, join or probe-triggered action. Packaged system service and
published-container lifecycle acceptance remain separate.

The [rootless container recipe](../../docs/testing-worker-container.md) now
verifies source-built evaluation images with private state, exec probes,
disabled/probes-only modes and explicit stop/restart. A separate container's
loopback cannot scrape this worker; network sharing grants diagnostics access
without granting its filesystem or operator credential. No image is published
and no worker port is exposed remotely by this recipe.

The optional `observability` build provides loopback Prometheus metrics and
startup/liveness/readiness probes with explicit runtime enablement, as described
in the [implemented local surface](../../docs/orishu-observability.md#implemented-local-surface).
No monitoring port is enabled by default. Local sampled OTLP export and the
documented mTLS monitoring proxy have scoped source-build evidence; cross-peer
tracing and full remote deployment acceptance remain incomplete under
[ADR 0017](../../docs/adr/0017-worker-operational-observability.md). The
[operator manual task](../../docs/tasks/document-worker-observability.md) tracks
the remaining deployment, collector and troubleshooting acceptance. Local
metrics/probes do not establish the complete M4 monitoring handoff.

The [monitoring incident runbook](../../docs/worker-monitoring-runbook.md)
provides bounded read-only checks for peer loss, local unreadiness and missing
telemetry. It distinguishes failed monitoring access from failed owner
supervision and preserves admission-recovery stop conditions; no probe or
counter authorizes an automatic restart or fresh join.

An [optional formation alert group](../../docs/testing-worker-prometheus.md#optional-formation-warnings)
supplies tested owner/admission/catch-up/timeout/capacity warnings using the
existing metrics. Enable it separately in Prometheus, adapt all job selectors
together and review its example thresholds. Existing trace-loss rules should
use the corrected per-counter expression; no worker rebuild or new feature
is needed for this rule change.

The [formation dashboard example](../../docs/testing-worker-dashboard.md)
provides 37 per-target snapshot panels and links to Prometheus history views.
It is served by Prometheus, not the worker. Tracing-off and unavailable
instruments remain explicit; no additional worker capability or port is added.

See the [project architecture](../../docs/architecture.md), [security
policy](../../SECURITY.md), and root [README](../../README.md).
