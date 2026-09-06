# Cluster-formation integration record

Status: **historical evidence; not an implementation task or acceptance ledger**

Parent: [Implement the operational cluster-formation PoC](implement-cluster-formation-poc.md)

This record preserves previously reported integration work and validation results.
It was separated during a documentation-only review; the checks below were not
rerun by that review. Entries are chronological and later entries may supersede
earlier gaps, counts or maturity statements. Use the parent task's current-gap
summary and acceptance criteria to plan work. New evidence should be recorded
against its checkpoint with a named test, command, result and limitation, rather
than adding another narrative that readers must reconcile.

## Historical integration notes

The following is a chronological integration record, not a current checklist.
Later entries supersede earlier statements that a particular adapter or route
is unwired. The current-gap summary above and checkpoint evidence requirements
govern planning; preserve the distinction between reported tests and checks
rerun during the current review.

- Core `MembershipPolicy` now separates the replicated lock from node-local
  admission configuration. Gossip and anti-entropy tests cover lock/unlock,
  ordering, conflicts, version exhaustion and admission re-checks; operator
  removal respects the lock while SWIM remains independent. Hash domains are
  version 2, with updated golden fixtures.
- Per-sender replay tracking now permits unseen requests within a bounded
  128-sequence window across independently reordered streams, while rejecting
  duplicates. Formation adoption drops old policy and replay state.
- Worker `peer::codec` bounds encoded lengths, nesting, collections and work
  before typed allocation, including duplicate-map checks and golden frames.
  `peer::read_frame` checks the prefix before allocation and bounds total read
  time through stream FIN; writes also have a deadline.
- Worker `peer::tls` negotiates `orishu-membership/2`, requires client key
  possession and pins the introducer certificate. Real loopback QUIC tests
  cover mutual authentication, wrong pins, missing client certificates and
  malformed/mismatched private identities. These modules are not yet wired
  into the worker executable; there is no operational cluster claim.
- Worker `credentials` implements bounded, owner-only Unix identity/operator
  token persistence, exclusive instance ownership and fail-closed reload.
  Tests cover restart, corruption, unsafe permissions and symlink rejection.
- `peer::session` now extracts TLS/ALPN facts and distinguishes applicants,
  pinned outbound introducers and admitted members. It checks assigned
  identity/certificate binding against current membership, rejects stale
  generations and rechecks identity blocks/removal. Member dialing supports
  membership-held fingerprint pins; real QUIC tests verify matching and wrong
  pins. The owner-side registry and network checks below now consume those
  bindings; executable listener orchestration is still pending.
- `peer::admission` now produces secret-free credential/source-network verdicts
  from literal-IP/CIDR policy and the transport-observed address. Invalid or
  oversized policy fails closed with an explicit completion, and mapped-address
  tests prevent an IPv4 rule being bypassed by IPv6 representation. The real
  QUIC admission test feeds that evidence back into the core and reaches its
  ID-allocation effect. The owner packet path now executes that bounded check
  for registered sessions, with source revalidation as described below.
- The bounded handshake DTO now binds request/acknowledgement roles over a
  separate bidi stream without an admission token. The real QUIC test drives
  handshake, core-generated JoinReq, credential evidence, ID allocation,
  encoded JoinAccepted and validated adoption by a second core. This remains
  an adapter integration test, not separate production worker drivers or the
  required three-process demonstration; connection orchestration and catch-up
  must still be connected.
- Handshake registration now enters the real owner through its bounded peer
  lane. Its owner-side registry caps total/provisional sessions, expires
  applicants, preserves process-unique session IDs and closes removed or
  old-generation bindings. Real QUIC tests cover owner-decided ACK, duplicate
  connection refusal, capacity/expiry, leave fencing and shutdown of registered
  and queued connections. This connects handshake binding to the owner, not
  the executable peer accept loop or post-handshake admission/send dispatcher.
- Registered sessions now recheck active network rules against QUIC's current
  source address during registration and owner sweeps. A shared bounded,
  allocation-free rule iterator supplies both this check and token-verification
  evidence. The real migration test rebinds a client to a blocked loopback IP
  and verifies owner-driven closure, later rule lifting and fail-closed invalid
  policy. The registered-packet owner path below now performs the same check
  immediately before decoding; sweeping sessions alone is insufficient.
- Raw registered-session packets now enter the bounded peer lane with transport
  and generation correlation. The owner revalidates the current registry and
  source, decodes the wire payload, and applies its core input in one turn.
  A session-addressed reliable effect is correlated to that request's reply
  stream. A real QUIC test exercises JoinReq-to-core-to-locked-refusal and rejects
  unknown sessions/malformed frames without membership changes. The executable
  peer listener is not enabled by this intermediate path.
- The runtime now moves its fresh standalone join token into the IO owner.
  Registered requests drive bounded token/source verification and correlated
  core completion inline, followed by core ID allocation and insertion/ACK.
  The real QUIC admission test covers lock/invalid-token rejection, successful
  insertion with independently validated joiner-core adoption, and rejection
  of the previous token after leave. Adoption clears prior token authority;
  target credential installation remains catch-up work. This proves an
  admission exchange; the sustained owner transport evidence is described below.
- Admission now promotes the inserted session; known-member reconnects bind
  against current membership. Member sends use real QUIC datagrams or bounded
  reliable tasks whose responses re-enter the owner. Missing routes and IO
  failures are counted, not fabricated as delivery or made terminal owner
  failures. The real-QUIC integration test runs two owners after validated
  adoption/reconnect, observes SWIM and reliable exchanges, and converges lock
  and unlock in both directions. The second owner is initialized from the
  adopted core by the test. Automatic dialing, the executable accept loop,
  production join commands, recovery and introducer catch-up remain required;
  this is not three independent worker processes starting and joining via CLI.
- `peer::exchange` now bounds concurrent reliable IO without an unbounded
  permit-wait queue, applies an operation-wide deadline including owner-reply
  waiting, and resets/stops cancelled exchanges. Datagram submission refuses
  stream-only/oversized input without fallback. Real QUIC tests exercise
  request/reply, overload, cancellation and permit release; the handshake/join
  integration test uses this pool for its handshake. Live dispatcher and
  owner-completion mailbox wiring remain incomplete.
- `peer::wire` maps all current membership message effects to explicit CBOR
  DTOs, checks session/envelope binding before typed payload allocation,
  separates admission tokens from core inputs, and measures actual snapshot
  bytes. Golden/hostile fixtures cover the envelope, delivery mapping, snapshot
  length and byte-budgeted datagram gossip. A real QUIC join-request test feeds
  the decoded request into the core's credential-verification effect.
- The executable now loads credentials, generates fresh formation/node IDs and
  serves a real versioned `ClusterSummary` through the shared client and CLI.
  Two-process tests verify independent startup, exclusive state ownership and
  restart identity changes. Plain TCP is rejected; remote summary reads require
  the operator token. The runtime truthfully reports introduction unavailable.
- Membership now lives in a serialized owner task; HTTP reads use a bounded
  watch projection and return unavailable if that owner stops. The initial
  standalone driver runs periodic probe/reconciliation commands, owns bounded
  monotonic timers, performs peer selection/ID allocation, rejects stale
  generation input and reserves a shutdown mailbox. Registered-request token
  checks, correlated session replies and registered-member sends now execute.
  This is not yet the complete driver required by
  slice 3. Structured event delivery, steady-state peer IO and end-to-end
  fairness under the live listener remain part of that integration.
- Owner ingress now separates 64 peer messages, 16 operator commands and 64
  effect/timer completions, plus reserved shutdown. Bounded processing quotas
  prevent continuously ready completion/control queues from starving queued
  peer input; due owner timers retain priority. A regression fills peer ingress
  and proves control/completion submission and processing still succeed. The
  live executor must still reserve/correlate completion capacity before issuing
  asynchronous work; callers may not discard an overloaded completion.
- Verification work can now reserve its completion slot before starting. The
  single-use handle fixes request/session/generation correlation and queues a
  negative verdict if dropped or its task is aborted. Tests exhaust all slots
  and exercise completion/cancellation without losing an outcome. The current
  local token check completes inline without asynchronous IO; any future async
  verifier must acquire these handles and own task deadlines. This primitive
  alone is not evidence of the complete admission workflow.
- Remaining work includes join/leave client DTOs and routes,
  session orchestration and admission-state catch-up, interrupted-join
  recovery, completion of driver integration, peer configuration and the
  remaining API/CLI routes,
  observability integration and the complete multi-process failure matrix.
  The remaining preflight choices must be recorded before their adapters land.
- The CLI now explicitly loads a bounded, private worker operator-token file
  and attaches its bearer credential through the shared client. Loader tests
  cover malformed/oversized files, unsafe permissions, links and FIFOs; the
  CLI process test exercises the actual serialized authorization header and
  redacted failure output. This closes credential-file input, not worker
  mutation authorization or the real three-worker operator journey.
- Unix socket lifecycle tests now verify active-listener protection, stale
  socket recovery, replacement/link preservation and unsafe-parent refusal.
  The real worker restart test reuses the same socket after both graceful
  shutdown and process loss. Fixtures explicitly create private directories;
  permissive temporary-directory defaults must not weaken production checks.
- The membership owner now accepts acknowledged lock/unlock intents on its
  existing bounded operator lane, checking both generation and formation at
  dequeue time. Tests cover applied-state replies, repeated intents, a queued
  request overtaken by leave, overload and requester cancellation. This is an
  internal authorized-adapter interface used by the identified HTTP path below.
- Versioned `LockRequest`/`LockReceipt` now connect the actual worker POST
  route, shared client's `set_lock`, and CLI lock/unlock commands. All mutations
  require the worker operator credential, including on Unix sockets. Exact
  retries recover historical outcomes without reapplying a superseded intent;
  conflicts and stale formation preconditions are rejected. The explicit PoC
  history limit is 1,024 outcomes per worker's formation lifetime, with no
  eviction/reexecution. Frame/deadline/concurrency limits and expiry semantics
  are recorded in the client protocol; asynchronous join operation tracking
  remains unresolved. Real process tests exercise malformed/unauthorized input
  and the shared client. `python3 scripts/check-formation-cli.py` exercises the
  production CLI lock/unlock/retry journey on one worker, not three-worker
  formation or peer policy convergence.

This evidence does not satisfy the final acceptance checklist. In particular,
two endpoints in one TLS test are not three production worker processes.

The production `peer::server` dispatcher now bounds inbound TLS/connection and
stream work and shares the owner's reliable-exchange pool. Real-QUIC tests
`dispatcher_handshake_and_owner_shutdown_close_all_connections` and
`silent_application_handshake_expires` cover the first-handshake deadline and
shutdown, including unregistered connections. The existing two-owner traffic
test now uses its registered stream/datagram loop. Run these with
`cargo test --locked -p orishu-worker --lib`. This adds adapter evidence only:
executable listener configuration, outbound dialing, operator-led join and
catch-up still prevent closure of the two-worker vertical-slice checkpoint.

Explicit single-address peer listener startup is now connected through the
typed file/env/CLI configuration and runtime supervision. Wildcard binds need
a usable explicit advertisement; concrete port-zero binds publish the actual
allocated port. Runtime tests verify endpoint advertisement and owner-driven
connection closure. Admission remains disabled in this intermediate executable
surface: join operations, dialing and catch-up still prevent vertical-slice
acceptance. This supersedes the listener-configuration gap above, not the
three-process evidence requirement.

Node inspection now reads a single identity through the owner's bounded control
lane and returns a versioned local-view resource through the production route,
shared client and CLI. Tests exercise known/unknown identities, unsupported
source modes, and remote authorization. `scripts/check-formation-cli.py` also
checks the actual `inspect` command. This does not implement membership listing,
direct remote-node queries or prove three-worker convergence.

Membership listing now shares that projection and reads four-record ordered
pages through the production route. The owner checks continuation formation;
the client validates page/source identity and ordering under bounded collection
and time limits. Pure/owner tests exercise multi-page traversal and stale/missing
formation rejection; process tests and the CLI journey exercise real listing,
filtering and remote authorization. This supersedes the listing gap above but
does not make operator paging a catch-up barrier or a frozen snapshot.

Privileged join-material retrieval now reads the owner-held token together with
its formation, introducer identity/fingerprint and peer endpoints. The client
and `token` CLI consume this versioned resource; raw token debug output is
redacted and rotation explicitly rejects. The real CLI journey verifies local
authorization, identity/pin binding, actual port-zero advertisement and token
absence from inspection. Export reports introduction unavailable while the
join/catch-up workflow remains unfinished; no successful join is claimed.

The CLI now prepares a versioned identified join input from a private bounded
JSON material file, with distinct source and target formations. Loader tests
cover privacy, links, size, endpoint validation and redacted errors. The old
raw-token CLI and automatic-leave claim are removed; the real CLI journey
checks that the unfinished executor fails explicitly without transmitting or
printing secret input. Wiring this envelope to the owner, operation tracking,
pinned connection and validated adoption remains the next integration work.

A secret-free bounded outbound bootstrap adapter now completes pinned TLS and
the initial handshake against the real dispatcher. Its target retains the
operator-supplied formation/node/fingerprint binding; ACK validation rejects
a mismatched introducer identity. The real-QUIC dial test covers wrong pins,
capacity and abandoned-connection cleanup. It never receives or sends the
token. Registering the completed handshake through the active owner operation,
then sending JoinReq and driving adoption/recovery/catch-up, remains unfinished.

Pinned introducer ACKs now enter the current owner through the bounded peer
lane, with source-policy checks, owner-assigned session IDs, generation fencing,
and provisional lifetime/capacity enforcement. The real dial test checks owner
registration and wrong-identity/stale-generation closure without changing the
joiner's formation or member count. The binding is JoinReply-only. This closes
session-registration plumbing, not active-operation correlation or JoinReq
execution; the operator join state machine remains the next required work.

The internal owner begin-join path now checks source formation/generation,
standalone participation, attempt exclusion and the registered introducer.
It executes core JoinReq effects through the bounded real transport, retaining
the secret only in shell state; replies re-enter the owner and validated
adoption reaches `catchingUp`. The real dial test now proves two-owner admission
and adoption from independent standalone models, plus stale-source rejection
and absence of introduction/credential readiness after adoption. Operator
operation tracking/routes, interrupted-join recovery, reconnect and complete
catch-up still block the public two-worker and three-worker checkpoints.

Lost-ACK classification now has a real-QUIC regression: the introducer inserts
the member, a test-only response-write fault discards its ACK, and retry
exhaustion leaves the joiner `joinUnresolved` rather than eligible for another
join. The source formation and remote insertion are preserved, transient secret
and send state are dropped, and another begin request is refused. This provides
ambiguity evidence, not outcome recovery; the identified recovery protocol and
operator procedure remain required before interrupted-join acceptance closes.

The versioned join-operation DTO and bounded worker-local tracker now distinguish
processing, possible admission, adoption and catch-up. Tests cover exact replay
after adoption, changed-request conflicts (including token changes), exclusive
active work, non-evicting 64-record history and invalid lifecycle transitions.
Only a request digest and secret-free status are retained. The tracker must
still be integrated at the serialized owner boundary with supervised execution,
cancellation, status routes and client polling; unit bookkeeping alone does
not close the operation/recovery or process-level acceptance gates.

Operation preparation/status now run at the serialized owner boundary. A
single-use preparation owns a reserved completion slot, so dropping a job or
its reply receiver records pre-admission failure without losing completion.
Successful pinned IO returns through that slot for lifecycle/ACK revalidation;
the owner records emission and adopted target identity. The real dial test now
checks tracked adoption and exact replay across formation change. Runtime job
supervision/deadlines, authenticated routes and client polling remain unwired;
these tests do not yet establish a public two-process operator join.

Prepared jobs now have a runtime executor with bounded task retention, shared
dial capacity, a 16-second outer deadline, retained outgoing endpoint ownership
and shutdown exclusion/cancellation/drain. The runtime test verifies failed
certificate pinning, retained exact replay, no membership changes and task
cleanup. Public authenticated submission/status routes and client polling still
remain before this can be exercised as a real operator join workflow.

Authenticated identified join submission/status routes and the shared client/
CLI are now connected. The real CLI journey verifies authorization, bad-pin
failure, polling, exact replay, stale-source/changed-request rejection and
secret-free status output. Processing acceptance is explicitly distinct from
adoption and completed catch-up. The executable still withholds introduction,
so this is public failure-path evidence, not the successful two/three-worker
formation gate or interrupted-admission recovery.

Missing owner-held formation credentials now produce a negative token verdict
instead of a terminal owner error. The real-QUIC
`real_join_packet_drives_owner_lock_token_and_admission_outcomes` regression
includes an otherwise eligible introducer without an installed token, verifies
`InvalidToken`, unchanged membership and continued owner availability. This
closes a fail-closed prerequisite, not explicit admission configuration or the
post-adoption catch-up readiness contract.

Explicit `spec.accepts.peers` / `ORISHU_ACCEPTS_PEERS` / `--accepts.peers`
configuration now enables standalone introduction, defaulting to false and
requiring a peer listener when enabled. Owner readiness requires a configured
role, advertised endpoint, safe participation phase and installed formation
token. Transitional phases refuse credential admission. The runtime test
`explicit_introducer_runtime_admits_but_adoption_withholds_readiness` exercises
real QUIC adoption between independently initialized runtimes and checks that
the adopted worker cannot export target credentials or claim introduction
readiness. Run with `cargo test --locked -p orishu-worker --all-targets`.
This runtime evidence alone does not establish public process acceptance or
completed catch-up.

The public two-process adoption checkpoint now has CLI evidence in
`scripts/check-formation-cli.py`. It starts isolated workers with duplicate
labels but distinct initial identities, explicitly enables introduction and
drives authenticated join/status through the real binary and listeners. It
checks target/assigned identities, matching membership ID sets and certificate
bindings, exact replay across adoption, secret-free status, withheld target
credential export, `catchingUp`/non-introducing status and bounded process/socket
cleanup. Build with `cargo build --locked -p orishu-worker -p orishuctl`, then
run `python3 scripts/check-formation-cli.py` on Unix with loopback sockets
available. The command passed for this increment; no stable liveness convergence,
complete catch-up, B-to-C introduction or recovery claim follows from it.

The admitted-member dial path now derives a bounded secret-free routing snapshot
from current membership and sends an assigned-identity handshake using the
recorded certificate pin. It shares bootstrap dial/exchange limits and returns
through owner-fenced member registration. The real-QUIC
`real_join_packet_drives_owner_lock_token_and_admission_outcomes` test now uses
this adapter when reconnecting after adoption, then verifies SWIM and replicated
lock traffic. This replaces manual TLS/request construction in that test, not
test-driven connection scheduling: automatic reconnect/backoff, simultaneous
dials and public steady-state convergence remain required.

Runtime maintenance now schedules canonical admitted-peer connections using a
bounded one-second cursor scan, shared dial budget and per-target exclusion;
generation changes and shutdown cancel old work. The lower assigned ID dials
when both advertise endpoints; unadvertised workers dial reachable peers. The
driver now includes suspected members in peer selection so restored sessions
can carry actual probes/refutations rather than becoming permanently idle.
The public CLI journey verifies lock/unlock convergence after adoption without
manually constructing a session, and verifies that a catching-up worker still
cannot mutate policy. The runtime adoption test additionally injects connection
loss through a `cfg(test)`-only registry close hook, preserving membership and
session-ID allocation, then verifies autonomous reconnect, policy convergence
and two live members. `cargo test --locked -p orishu-worker --all-targets` and
`python3 scripts/check-formation-cli.py` passed for this increment. This does not
close the three-worker simultaneous-dial, long-partition, malformed catch-up,
ejection or overload matrix; it does not grant introduction readiness.

The public baseline content contract is now specified under peer connection
maintenance. `peer::catchup` freezes the complete policy/blocklist/tombstone
projection into bounded canonical pages and verifies formation/source/snapshot,
ordering, totals and an aggregate digest before exposing records. Tests cover
explicit empty collections, immutable capture across policy changes, pagination,
missing/reordered/duplicate/mixed pages, digest/size rejection, lifted blocks
and cleared tombstones. Run `cargo test --locked -p orishu-worker --lib
peer::catchup`. This is content validation only: source retention/expiry,
authenticated peer routes and wire negotiation, atomic core merge, credential
release/installation and completion publication remain required. No worker
readiness changes follow from these helper tests.

Atomic domain installation now has the versioned core
`InstallAdmissionBaseline` command and one correlated applied/rejected outcome.
It stages the normal merge rules against a bounded clone, retaining newer local
facts and rolling back all state/gossip changes on malformed records, conflicts
or capacity failure. Self-removal is explicit; local admission settings and
credential readiness are not changed. `baseline::tests` covers replay, late
failure rollback, stale/empty source views, wrong formation, duplicate keys,
union capacity and self-removal. The worker receiver now produces this command
value only after content completeness validation, retaining formation/snapshot
binding. Authenticated transfer, owner outcome handling/ejection and target
credential installation remain required before catch-up can complete.

`peer::catchup::store` now retains at most four immutable baselines for 30
seconds, with monotonic snapshot IDs, requester/generation binding and no live
eviction. Exact retries preserve bytes and expiry; continuation/finish rejects
skipped pages, wrong roots, unrelated requesters, stale generations and expired
snapshots. Identity checks reuse the registered-session member validator, and
new identity exclusions invalidate retention. Tests cover frozen retry across
source policy changes, expiry, capacity, paging completion and revocation.
This is source storage evidence, not an authenticated route or credential
release: owner readiness/source-network checks and network integration remain
required before a newly admitted worker can complete catch-up.

Profile 3 now connects source-side admission-state request/reply handling to
the real bounded peer dispatcher and serialized owner. The owner validates
current member/session/source restrictions and readiness before serving retained
pages or releasing its token after confirmation. The real-QUIC admission/traffic
test now fetches the descriptor/pages, verifies complete content and rejects
premature confirmation before receiving the expected credential. Wire tests
cover readiness refusal, identity mismatch, size and reply correlation. ALPN
changes from `orishu-membership/2` to `/3` with no downgrade; Merkle hash domains
remain version 2. Receiver scheduling, core-outcome handling, atomic credential
installation and the B-introduces-C process checkpoint remain incomplete.

`peer::catchup::client` now performs a complete receiving-side transfer over an
already registered connection: source certificate binding, correlated descriptor,
sequential page validation, then snapshot/root-bound credential retrieval. It
has a 25-second overall deadline, 128 KiB pre-allocation reply cap and shares the
existing exchange budget. The real QUIC admission/traffic test now serves 40
blocklist records over multiple pages and verifies the received baseline/token
and wrong-pin rejection. The resulting completion is private shell data, not
readiness: receiving-owner scheduling, attempt/generation fencing, core-outcome
handling and explicit credential installation remain required. Full malformed,
expired and interrupted transfer conformance is not established by this test.

Receiving-owner preparation/completion now reserves completion capacity, fences
attempt/generation/source-session identity and limits catch-up to three prepared
attempts within 90 seconds of adoption. Cancellation records failure; explicit
retry transitions the existing operation back to catching up. A validated
completion applies the atomic core command and installs its shell token only
after acceptance and local identity/network-policy checks; self-removal closes
sessions and prevents installation. The runtime adoption test drops one job,
observes retained failure, retries through real QUIC and verifies `joined`,
introducer readiness and the same formation token. This is owner/runtime
evidence, not automatic runtime scheduling or the public B-introduces-C gate;
those and the complete stale/ejection/catch-up fault matrix remain required.

Automatic catch-up is now enabled at production startup with one supervised
transfer and one-second eligibility polling, retaining the owner's attempt and
deadline bounds. The runtime adoption test preserves incomplete-readiness and
cancelled-attempt evidence, then starts the scheduler twice to verify idempotent
startup and automatic retry to `joined`. The public CLI harness now starts three
independent workers, drives A-admits-B then B-admits-C, and verifies historical
source identities, assigned IDs, fingerprints, exact replay, live convergence
and lock/unlock through different entry nodes. No manual peer session or catch-up
invocation drives that process journey.

Validation for this increment: `cargo build --locked --offline -p orishu-worker
-p orishuctl`, `cargo test --locked --offline -p orishu-worker --all-targets
--quiet` (81 tests), `cargo clippy --locked --offline -p orishu-worker --all-targets
-- -D warnings`, and `python3 scripts/check-formation-cli.py` passed. The harness's
first run exposed an incorrect expected CLI summary key; it was corrected to
the actual `alive`/`nodes` output and the full journey rerun successfully.
This is happy-path handoff evidence, not lost-ACK recovery, complete lifecycle
or fault conformance. Failure artifacts and stale-completion/ejection cases
remain required; no observability implementation is claimed.

The owner now handles self-removal from ordinary gossip as well as an atomic
baseline through one ejection fence. It suppresses the transition's entire
effect batch, advances generation, clears credentials/pending catch-up, closes
sessions and stops timers, sends, dialing and periodic membership work. The
real-QUIC `real_join_packet_drives_owner_lock_token_and_admission_outcomes`
regression now sends a self-tombstone in an authenticated datagram and checks
ejection, connection closure, unavailable introduction/catch-up and no further
core transitions from old/new-generation probe submissions. The runtime
`ejection_fences_prepared_catchup_and_its_late_cancellation` test applies a
baseline at the owner boundary while a prepared job is held, then drops its
late completion and verifies the owner remains available and ejected with a
retained catch-up failure. Both focused tests passed. This is not yet public
three-process ejection/restart exclusion or a late successful-transfer fault;
those lifecycle cases remain required.

Validation after the ejection fence: the worker all-target suite passed 82
tests, worker all-target Clippy passed with warnings denied, the rebuilt
three-worker CLI handoff passed, and scoped formatting/whitespace checks passed.
`make docs-check` still fails on the unrelated existing `TODO.md` link
`./docs/adr/0013`; it is not recorded as passing. The public leave route is
still unsupported. Review found that its prerequisite owner path replaced the
model and fenced sessions before executing departure effects; the following
increment corrects that identity boundary.

The owner now captures the bounded old-generation notification routes before
the core leave transition, then submits only validated old-sender `Leave`
datagrams before retiring those sessions. Departure effects are consumed there,
never encoded/routed as replacement-formation work. Missing routes or failed
submission remain non-fatal bounded diagnostics, with no promise of receipt or
flush before closure; SWIM remains the fallback. The production encoding helper
has a regression using real core leave effects and CBOR field decoding, proving
old formation/sender identity after model replacement and rejecting a mismatched
target or unrelated message. The worker all-target suite passed 83 tests and
Clippy passed with warnings denied. This is internal owner/codec evidence, not
public leave acceptance. Identified/authenticated leave routes, retained retry
outcomes, CLI wiring and real departure/loss convergence remain required.

Identified voluntary leave is now connected through typed request/receipt,
serialized owner, authenticated bounded POST route, shared client and CLI.
Process-lifetime history retains 64 successful receipts including no-ops,
checks exact replay before current-formation preconditions and never evicts
into reexecution. New stale/conflicting requests refuse; client cancellation
does not revoke acceptance. Leave from joined/catching-up/ejected state creates
fresh random identities/token and fences old work; joining/unresolved admission
refuses pending its separate recovery contract. The current display label and
private certificate/operator credential are retained. No compute drain or
artifact transfer is claimed.

The owner tests verify receipt replay across identity change, lost caller,
lock-independent departure, no-op, stale/conflicting input and full history.
Real API tests exercise local authorization, exact no-op replay, duplicate
credentials, oversized/malformed bodies and unsupported content/preconditions.
The CLI journey now leaves C and checks fresh identities/token, certificate
retention, exact replay, stale/conflict/no-op and eventual dead state of its old
identity at both survivors. Its first ten-second convergence deadline was
shorter than the default size-scaled suspicion timeout (30 seconds at three
members); the corrected 60-second bounded poll passed. This does not prove
that the lossy announcement arrived, nor does it close deliberately dropped
announcement or interrupted-leave races. Those remain required. The journey
also explicitly readmits C after departure visibility, verifies new assigned
identity with its retained certificate and old dead history, and replays the
old leave request after readmission without changing current membership.

Validation: worker all-target tests passed (85), shared `orishu` tests passed
(93), CLI tests passed (7), scoped all-target Clippy and formatting passed,
and `make docs-check` passed. The rebuilt public CLI journey passed with leave
and readmission. This changes the early client API from parameterless `leave`
to `leave(&LeaveRequest)` and the CLI requires explicit formation/operation
IDs. No full-workspace or observability-feature acceptance is claimed.

The process harness now keeps bounded combined worker-log tails in memory and
retains only a bounded recent non-token CLI journal. Failure writes private
redacted per-worker logs and a versioned report with scenario, exception type,
process IDs and observed exit status; success writes no bundle. State directories,
credential files, join material and command arguments are never archived.
Invalid credential parsing withholds raw evidence rather than risking export.
Tail truncation discards a partial first line, token/key encodings and long
opaque strings are scrubbed, and files/directories have tested private modes.
Identity hex values in raw output are deliberately redacted too.

`python3 scripts/test_formation_evidence.py` passed four tests, including real
subprocess output flood, seeded secrets/encodings, limits, permissions and
fail-closed invalid identities. The real harness's
`--inject-failure-after-startup` mode produced the expected nonzero exit and
private bundle; all three reported worker PIDs were absent after cleanup.
The normal CLI leave/readmission journey then passed with capture enabled.
`make test-formation` now provides one build/evidence-tests/process-journey
entry point. The deliberate failure tests evidence plumbing only, not lost
ACKs, partitions, ejection or other protocol faults. Those matrix rows remain
required; this does not provide production observability exporters.

The CLI harness now exercises joined-worker SIGKILL, observes suspicion then
death at survivors, restarts the killed worker with the same private directory
and Unix socket, and verifies fresh standalone IDs/token with retained
certificate/operator credentials. It explicitly readmits the restarted worker
and checks all three views against the exact five-record identity/fingerprint/
liveness map: two dead historical IDs and three live current IDs. This uses
ordinary process signals and public CLI routes, not a worker fault endpoint.

One run reached the initial ten-second final reconciliation deadline with the
third observer still missing the restarted node's record; all three processes
remained responsive and the private failure bundle captured the mismatch.
Another fresh run passed the exact assertions. The final wait now permits 60
seconds to cover multiple five-second reconciliation rounds and ten-second
round timeouts; the repeat passed. No runtime convergence fix or ten-second
SLO is claimed. The shared evidence helper supports restart only after exit
with the same state directory, reuses a bounded tail, and reaps a child if its
log-reader thread cannot start. Its six tests and `make docs-check` passed.
Certificate-excluded restart, deliberate partitions, interrupted admissions
and the remaining matrix are still required; ordinary restart/readmission
does not establish those guarantees.

Interrupted-admission review found that certificate duplication and policy/
capacity races could pass the final node-ID allocation boundary. The core now
refuses `AlreadyAdmitted` for a certificate already held by an alive/suspected
member in its local view and rechecks every locally held gate immediately
before insertion, after allocation. Dead records remain valid history and do
not by themselves prevent a new assigned identity; active exclusions still do.
This prevents duplicate local insertion, not partition-wide reservation or
lost-ACK outcome recovery.

Core admission regressions cover two outstanding allocations racing on one
certificate, the final local capacity slot, a new lock and a new fingerprint
block; only the first valid admission inserts. Alive/suspected versus dead
history is also covered. Worker codec tests freeze `alreadyAdmitted` CBOR and
the complete NACK envelope, and the real QUIC owner admission test verifies
the same refusal with unchanged membership. The core all-target suite passed
200 tests plus benchmark smoke cases, including dependency-purity checks;
the final worker suite passed 86 tests including the additional real-wire case.
The membership doctest, scoped Clippy/formatting, docs validation and rebuilt
public CLI leave/crash/restart/readmission journey also passed. Interrupted-join
recovery remains required.
