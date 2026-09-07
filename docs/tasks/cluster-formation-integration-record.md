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
is unwired. The parent task's current-gap summary and checkpoint evidence requirements
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

## Dated lifecycle and recovery checkpoints

These reports were moved unchanged from the parent task during a
documentation-only review. Their dates, failures, test counts and limitations
remain historical; relocation does not revalidate them. Current acceptance
disposition belongs to the [conformance ledger](cluster-formation-conformance.md).

#### Catch-up lifecycle conformance — verified 2026-09-05

Command: `cargo test --locked --offline -p orishu-worker --lib runtime::tests -- --nocapture`
passed all 10 runtime tests on Unix with loopback socket access. The following
five cases were added in this increment; names are in `runtime::tests`.

| Named test | Production boundary and assertion | Result / limitation |
| --- | --- | --- |
| `leave_fences_successful_catchup_completion` | Real QUIC baseline/credential fetch, then owner leave before successful completion delivery; fresh standalone IDs/token remain unchanged | Passed; runtime leave path, not a lost HTTP receipt test |
| `ejection_fences_successful_catchup_completion` | Same verified receiver result delivered after self-removal; no token/readiness restoration or further core transition | Passed; ejection injected as an owner baseline command, not a public removal route |
| `shutdown_fences_successful_catchup_completion` | Reserved completion held while runtime/owner shuts down, then released; closed owner cannot revive | Passed; bounded shutdown with held successful result, not every in-flight network shutdown phase |
| `source_session_loss_refuses_successful_catchup_then_retries` | Retire the authorized session without changing formation generation; refuse its late result, then automatically fetch/install through a fresh attempt | Passed; real transport/receiver and owner registry, not a three-process network partition |
| `exhausted_catchup_attempts_remain_non_introducing` | Cancel three reserved preparations; further preparation and automatic scheduling cannot reset the budget or install credentials | Passed; attempt limit, not elapsed 90-second adoption deadline |

The scheduling barrier is compiled only in tests. It waits after the normal
receiver verifies all pages and confirmed credentials, before the normal
reserved completion send; it cannot fabricate a `Completed` value. FIFO
control replies establish that late completion was processed before assertions.
No production protocol, dependency, credential format or runtime behavior was
changed by this increment. It does not close interrupted-admission recovery,
the full process fault matrix or the observability companion tasks.

Additional checks for this increment passed:

- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`
  (91 tests: 77 library, 9 binary and 5 integration).
- `cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings`.
- `cargo fmt -p orishu-worker -- --check`.
- `cargo test --locked --offline -p orishu-membership --test dependencies --quiet`
  (3 dependency-purity/budget tests).
- `make docs-check` (86 Markdown files) and `git diff --check`.

The full workspace, public CLI journey and observability-feature matrix were
not rerun for this test-only increment; earlier results remain historical.

#### Catch-up deadlines and partial transfers — verified 2026-09-05

| Named test | Command filter / production boundary | Result / limitation |
| --- | --- | --- |
| `runtime::tests::adoption_deadline_refuses_successful_catchup_completion` | `adoption_deadline_refuses`; real QUIC fetch and reserved owner completion after adoption expiry | Passed; test-only owner control ages the adoption timestamp to 90 seconds without retiring the valid session or changing generation; this isolates the deadline guard, not wall-clock scheduling latency |
| `peer::catchup::client::tests::expired_continuation_refuses_partial_baseline_and_allows_retry` | `peer::catchup::client::tests`; actual source store/wire handler and receiver over mTLS QUIC | Passed; controlled source time expires the lease after page zero; production continuation returns `unavailable` |
| `peer::catchup::client::tests::truncated_page_frame_releases_exchange_capacity_for_retry` | Same receiver filter; declared frame length exceeds delivered final-page bytes | Passed; transport failure is bounded and a fresh transfer succeeds on the same connection/pool |
| `peer::catchup::client::tests::cross_snapshot_page_refuses_partial_baseline_and_allows_retry` | Same receiver filter; final page names another snapshot | Passed; no complete baseline or credential request escapes |
| `peer::catchup::client::tests::incorrect_digest_never_requests_credential_and_allows_retry` | Same receiver filter; descriptor root disagrees with both complete pages | Passed; digest failure precedes credential confirmation |

Run each filter with
`cargo test --locked --offline -p orishu-worker --lib FILTER -- --nocapture`.
All four receiver cases fetch two pages, fail after partial progress without
requesting a credential, then complete a fresh transfer using the same QUIC
connection and a one-slot exchange pool. The test source uses admitted-member
fixtures and the real retention/resource handler; it does not run the production
handshake registry, receiving owner or CLI. The runtime deadline case separately
checks retained `catchUpFailed`, target identity, no introduction/token, and
refusal to prepare another attempt. These are test-only seams and no production
behavior, dependency or persisted/wire format changed.

Final validation for this increment passed:

- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`
  (96 tests: 82 library, 9 binary, 5 integration).
- `cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings`
  and `cargo fmt -p orishu-worker -- --check`.
- `cargo test --locked --offline -p orishu-membership --test dependencies --quiet`
  (3 tests), `make docs-check` (86 Markdown files), and `git diff --check`.

The full workspace, public process journey and optional observability-feature
matrix were not rerun. Interrupted-admission recovery is the next implementation
increment; the wider process fault matrix and operational handoff remain open.

#### Interrupted admission — bounded core rebinding, verified 2026-09-05

The core now accepts `RebindJoin` only for a matching pending session/target
and a distinct replacement session. It consumes the next existing retry,
cancels the old timer, fences old-session replies and cannot restart an
exhausted attempt. A duplicate `BeginJoin` no longer overwrites a pending
attempt. Early refusal/redirect retries also cancel their superseded timer.

`cargo test --locked --offline -p orishu-membership --test join --quiet`
passes 25 tests, including:

- `reconnect_preserves_join_budget_and_fences_old_timer_and_reply`;
- `stale_or_retargeted_reconnect_and_duplicate_begin_do_not_reset_a_join`;
- `repeated_reconnects_exhaust_the_original_join_budget_without_resurrection`;
- the extended `a_rejection_backs_off_and_retries` timer regression.

These are pure transition tests, not reconnect or lost-ACK acceptance. At this
checkpoint the worker redial integration below had not yet been verified.
Introducer attempt identity, accepted-assignment retention/replay, retired
assignment outcomes and real lost-ACK recovery tests remain required. No wire
profile or persisted format changed; downstream exhaustive matches over the
public `Command` enum must handle the new local variant.

The combined core/worker all-target test command
`cargo test --locked --offline -p orishu-membership -p orishu-worker --all-targets --quiet`
passed 203 core tests plus benchmark smoke cases and 96 worker tests. Scoped
all-target Clippy with `--locked --offline` and warnings denied, scoped format
checks and `git diff --check` passed. Full-workspace, public CLI and optional
observability acceptance have not been rerun for this increment.

After extending the refusal-timer assertion, the 25-test join suite and
`cargo test --locked --offline -p orishu-membership --doc --quiet` (1 doctest)
passed again. `make docs-check` passed for 86 Markdown files.

#### Pending-join redial — runtime integration, verified 2026-09-06

Production peer maintenance now starts one idempotent pending-join redial
supervisor. The owner retains the public operation's original secret-free
route/pin and permits one job at a time, at most eight preparations while
core retries remain. Each preparation reserves completion capacity before IO;
cancellation consumes the reconnect budget and clears the pending reservation.
Generation, previous session, reconnect ordinal, participation and the original
target binding are rechecked before invoking `RebindJoin`. The job contains no
token; the normal owner send path alone attaches it after revalidation.
The scheduler reuses the existing endpoint, dialer, shared IO budget and
handshake deadlines, with one-second polling/preparation waits and a 16-second
outer job deadline. Lifecycle changes and shutdown cancel supervised jobs.

| Named runtime test | Production boundary | Result / limitation |
| --- | --- | --- |
| `pending_join_redials_original_introducer_after_pre_insertion_disconnect` | Drop a real decoded JoinReq connection before owner insertion, then automatic pinned redial, original operation adoption and catch-up | Passed; pre-insertion transport recovery, not accepted-outcome replay |
| `cancelled_join_reconnect_jobs_release_reservation_without_resetting_budget` | Hold/cancel eight real owner preparations; one-active-job guard and automatic scheduling cannot reset the budget | Passed; preparation/cancellation cap, not eight timed-out network dials |
| `shutdown_fences_successful_join_reconnect_handshake` | Complete a real pinned replacement handshake, shut down with its reserved result held, then release it | Passed; no owner revival or remote insertion after shutdown |

Run each named filter with
`cargo test --locked --offline -p orishu-worker --lib FILTER -- --nocapture`.
Fault controls are compiled only in tests; no network fault endpoint or protocol
profile change was added. Introducer attempt identity, retained acceptance and
lost-ACK recovery remain the next required integration boundary.

Final validation passed:

- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`
  (99 tests: 85 library, 9 binary, 5 integration).
- `cargo clippy --locked --offline -p orishu-membership -p orishu-worker --all-targets -- -D warnings`
  and `cargo fmt -p orishu-worker -p orishu-membership -- --check`.
- `cargo test --locked --offline -p orishu-membership --test dependencies --quiet`
  (3 tests).
- `cargo build --locked --offline -p orishu-worker -p orishuctl`, then
  `python3 scripts/check-formation-cli.py` (public handoff, leave, crash/restart
  and readmission journey). The build retains the existing `proc-macro-error2`
  future-incompatibility warning; it did not fail.
- `make docs-check` (88 Markdown files) and `git diff --check`.

Full-workspace and observability-feature acceptance were not run. The public
journey remains a successful operator flow, not the full fault matrix.

#### Accepted-assignment replay — profile 4, verified 2026-09-06

The shell now generates one fresh attempt ID per BeginJoin and preserves it
across redial/retry. The introducer records at most 1,024 accepted requests per
local formation lifetime, keyed by authenticated fingerprint and attempt ID,
with a canonical public-request digest and assigned node ID. Capacity is checked
before insertion and the acceptance is recorded before its ACK. Records never
evict into reexecution; current authorization and member liveness/exclusion
checks precede replay. No credential/snapshot enters the ledger or core.

`runtime::tests::lost_join_ack_recovers_original_assignment_after_redial`
passes through the real runtime/QUIC path: discard acceptance after insertion
and ledger commit, close the connection, redial automatically, recover the
original ID, and complete catch-up. Exact operator replay remains the same
operation and the introducer keeps exactly two members.

The extended
`peer::registry::tests::real_join_packet_drives_owner_lock_token_and_admission_outcomes`
passes same-connection replay after promotion without another core transition;
wrong token, changed body and a new attempt on an admitted session refuse over
real streams. Wire tests check attempt-ID CBOR, credential/sequence exclusion
from the request digest, and changed public-body detection. The ledger capacity
test fills all 1,024 entries, preserves exact replay and refuses conflict/overflow
without eviction. Run the named tests with
`cargo test --locked --offline -p orishu-worker --lib FILTER -- --nocapture`.

Compatibility: ALPN is now `orishu-membership/4`; required `attemptId` changes
JoinReq, so older PoC binaries must be rebuilt/restarted together. Membership
protocol version 1 and Merkle hash version 2 remain unchanged. The prior-profile
TLS rejection test now offers profile 3. Operator manuals document replay
capacity and distinguish live-assignment recovery from unresolved outcomes.
Retired/excluded assignments, introducer loss/restart, unavailable ledger
recovery and the public process fault matrix remain incomplete acceptance work.

Final checks passed:

- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`
  (101 tests: 87 library, 9 binary, 5 integration).
- `cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings`
  and `cargo fmt -p orishu-worker -- --check`.
- `cargo test --locked --offline -p orishu-membership --test dependencies --quiet`
  (3 tests).
- `cargo build --locked --offline -p orishu-worker -p orishuctl`, followed by
  `python3 scripts/check-formation-cli.py` against profile-4 binaries: public
  handoff, leave, crash/restart and readmission passed. The existing
  `proc-macro-error2` future-incompatibility warning remains.
- `make docs-check` (88 Markdown files) and `git diff --check`.

The full workspace and optional-observability matrix were not rerun. The
ordinary three-worker journey is not the deliberate lost-ACK process fault test;
that fault currently has two-runtime, real-QUIC evidence as described above.

#### Retired/excluded assignment replay — verified 2026-09-06

The extended
`peer::registry::tests::real_join_packet_drives_owner_lock_token_and_admission_outcomes`
now checks the following additional cases through the production owner and
bounded codec:

| Scenario | Required assertion | Result / boundary |
| --- | --- | --- |
| Lock after acceptance | Exact replay retains the assigned ID and lock, without another core transition | Passed over the promoted connection's real stream |
| Remove the accepted member | A fresh applicant handshake cannot replay the removed ID; a fresh attempt still receives `tombstoned` | Passed; owner removal command, then real reconnect/JoinReq streams |
| Block the accepted fingerprint | Exact replay refuses; a fresh attempt still receives the fingerprint `blocklisted` reason | Passed; owner block command, observed old-session retirement, then real reconnect/JoinReq streams |
| Mark the accepted ID dead through self-departure | Exact replay cannot resurrect it; fresh admission assigns another ID and retains old dead history | Passed; serialized owner datagram input for departure, then real reconnect/JoinReq streams |

Command:
`cargo test --locked --offline -p orishu-worker --lib real_join_packet_drives_owner -- --nocapture`.
Fresh applicant handshakes succeed in these cases, so replay refusal cannot be
attributed merely to the already-retired old connection. No new production
behavior, schema or protocol profile changed in this test increment. Public
certificate-excluded restart/ejection, unavailable replay-state diagnostics,
and a bounded operator recovery procedure remain required; these negative
checks do not establish that operator workflow.

Final validation passed: `cargo test --locked --offline -p orishu-worker
--all-targets --quiet` (101 tests: 87 library, 9 binary, 5 integration),
`cargo clippy --locked --offline -p orishu-worker --all-targets -- -D warnings`,
`cargo fmt -p orishu-worker -- --check`, `make docs-check` (88 Markdown files),
and `git diff --check`. The test count is unchanged because the existing
parameterized wire test gained cases. Full-workspace, public CLI journey and
observability-feature checks were not rerun for this test-only increment.

#### Retained admission correlation — verified 2026-09-06

JoinOperation resource version 2 now includes an explicit nullable
`recoveryReference` containing the peer attempt ID, local applicant fingerprint
and original introducer ID/pin. The serialized owner binds it in the admission
start turn and retains it independently of transient IO. It cannot be replaced
by a changed reference. This is correlation only: issuer-side inspection,
unavailable-history outcomes and the operator recovery/stop procedure remain
required. JoinRequest/JoinMaterial version 1 and peer profile 4 are unchanged;
worker and operator clients must be rebuilt together for the resource change.

- `join_operations::tests::recovery_reference_is_immutable_retained_and_required_on_the_wire`
  passes JSON roundtrip, required explicit null, rejected old schema, immutable
  binding, unresolved lifecycle retention and secret exclusion checks.
- `runtime::tests::lost_join_ack_recovers_original_assignment_after_redial`
  now also verifies the original reference and local certificate through real
  QUIC recovery and exact operator replay. Run these filters with
  `cargo test --locked --offline -p orishu-worker --lib FILTER --quiet`.
- `cargo test --locked --offline -p orishu-worker --all-targets --quiet`
  passed 102 tests (88 library, 9 binary, 5 integration).
- `cargo test --locked --offline -p orishu --lib --quiet` passed 93 tests.
- `cargo build --locked --offline -p orishu-worker -p orishuctl`, then
  `python3 scripts/check-formation-cli.py` passed the public three-process
  handoff/leave/crash/restart/readmission journey. New assertions check explicit
  null in the initial version-2 receipt and the adopted reference's identities
  through CLI JSON; exact replay retains the same status. This is not the
  deliberate lost-ACK process fault or the unresolved operator procedure.
  The build retains the existing `proc-macro-error2` future-compatibility warning.
- Scoped all-target Clippy for `orishu` and `orishu-worker` with `--locked
  --offline` and warnings denied, scoped format checks, `make docs-check`
  (88 Markdown files) and `git diff --check` passed.

Full-workspace and optional-observability checks were not run for this increment.
This does not close interrupted-admission recovery or formation conformance.

#### Issuer-side admission inspection — verified 2026-09-06

The operator-only read-only admission inspection now connects a version-1
shared request/report, bounded owner lookup, HTTP handler, library client and
`orishuctl admission-inspect`. It reports wrong issuer, absent retained record,
current member or retired/restricted assignment. The report echoes correlation
and actual source identities; it never authorizes retry or turns missing
history into proof of non-insertion. Request size is 4 KiB, handler deadline
five seconds, and the handler shares the 16-request inspection budget.
No peer profile or persisted format changed; the new client trait method and
response variant require downstream implementations/matches to be updated.

`runtime::tests::lost_join_ack_recovers_original_assignment_after_redial`
now verifies issuer lookup of the original assignment through the serialized
owner after real QUIC recovery. The existing full-capacity ledger test checks
retained lookup and absent fingerprint/attempt pairs without admitting or
evicting records. These tests do not establish a public unresolved recovery
procedure or cluster-wide absence/exclusion proof.

`cargo test --locked --offline -p orishu-worker -p orishu -p orishuctl
--all-targets --quiet` passed 202 tests: 93 shared-library, 102 worker and
7 CLI tests. Scoped Clippy with the same three packages, all targets and
warnings denied passed, as did scoped format checks, `make docs-check`
(88 Markdown files) and `git diff --check`. The existing `proc-macro-error2`
future-incompatibility warning remains. Full-workspace and observability
acceptance were not run. Operator recovery/stop workflow acceptance remains
open even with diagnostic lookup available.

After `cargo build --locked --offline -p orishu-worker -p orishuctl`,
`python3 scripts/check-formation-cli.py` passed the public three-process
journey. New CLI assertions cover current assignment, absent attempt, wrong
issuer, retired assignment after voluntary leave and wrong issuer after its
restart, with exact echoed identities and unchanged repeated reports. Real
Unix HTTP checks reject missing operator authority, a join token used as the
credential, malformed/oversized bodies, content encoding, query parameters and
unsupported preconditions. This remains the ordinary handoff/lifecycle journey,
not the deliberate lost-ACK process fault or full overload conformance.

#### Recovery runbook and report correlation — verified 2026-09-06

The linked recovery runbook specifies current-versus-historical identity and
participation checks, a finite 300-second operator observation budget, unchanged
pending-attempt recovery and explicit stop conditions. Its budget is grounded
in current 183-second default retry windows, 15-second initial handshake and
90-second catch-up limits; it is a workflow cutoff, not a runtime completion
guarantee. Runbook documentation is not independent-process fault evidence.

The client now rejects a non-`wrongIssuer` report whose actual source
formation/node contradict the requested issuer. The new
`client::http_client::tests::admission_inspection_correlates_request_and_issuer_but_preserves_unknown_outcomes`
test exercises eight actual HTTP/CBOR response cases: valid current member,
changed attempt, changed source formation, changed source node, explicit wrong
issuer after restart, unavailable record, retired assignment and unsupported
schema. The transport fixture does not stand in for a worker process.

`cargo test --locked --offline -p orishu --all-targets --quiet` passed 94 tests;
scoped all-target Clippy with warnings denied, format check, `make docs-check`
(89 Markdown files) and `git diff --check` passed. Worker/CLI public journey,
full workspace and observability matrix were not rerun for this client/runbook
increment. Deliberate interrupted-admission process acceptance remains next.

#### Independent-process ACK loss — partial evidence, failures open 2026-09-06

`formation-fault-test` is an explicitly selected development-only worker
feature, absent by default and compile-rejected without debug assertions. Its
startup-only switch arms the existing one-shot post-insertion/ledger-commit
ACK-loss hook. It requires loopback peer binding, Unix clients and an explicitly
supplied state directory. No network fault-control route is introduced.
`make test-formation-lost-ack` builds into `target/formation-faults` so fault
artifacts do not replace ordinary operator binaries.

The public harness now requires an observed fault marker whose original
assigned ID matches the recovered version-2 status, exact operation replay,
issuer inspection and membership map. Its `--admission-only` diagnostic subset
passed across independent OS processes, including graceful shutdown:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl \
  --lost-join-ack --admission-only
```

**Full scenario failed and remains open.** Two runs without `--admission-only`
passed recovery/handoff/churn assertions but failed final graceful shutdown;
worker A reported an owner failure after SIGTERM (worker C also did on one
run). Two subsequent runs stopped earlier at the unchanged three-worker live
convergence deadline. No deadline was increased and no success criterion was
removed. The diagnostic subset is not a substitute for those failing rows or
the mandatory unresolved/issuer-loss stop journey. A temporary, feature-only
`[DEBUG-formation-shutdown]` classification in the worker supervisor was retained
for diagnosis at this checkpoint; it has since been removed with the supervision
race regression recorded below.

Default and feature-enabled worker suites each passed 103 tests (88 library,
10 binary, 5 integration). Feature-enabled all-target Clippy with warnings
denied passed. `cargo rustc --locked --offline -p orishu-worker --lib --features
formation-fault-test -- -C debug-assertions=no` failed with the intended
compile-time rejection (negative guard check, not a successful build).
Full-workspace and observability acceptance were not run. Preserve these
failures at handoff rather than reporting the new process fault as accepted.

#### Acknowledged-shutdown supervision race — verified 2026-09-06

The error previously printed as an owner failure was reproduced as
`PeerAdapterUnavailable`. Runtime supervision could observe peer-listener exit
after shutdown published `Stopping` and acknowledged the request, but before
the real owner dropped its published view. It incorrectly treated the still
readable view as proof that the listener failed while running, then returned a
synthetic failure even when the owner completed successfully.

`runtime::tests::peer_listener_exit_during_acknowledged_shutdown_preserves_owner_success`
failed before the fix and passes afterward. A test-only gate holds the real
owner after its shutdown acknowledgement, and the production listener-exit
branch is polled in that interval before cleanup is released. Supervision now
recognizes published `Stopping` and preserves the actual owner task result.
`runtime::tests::unexpected_peer_listener_exit_still_fails_and_stops_owner`
verifies the running-state failure path remains intact. No error category is
blanket-ignored and no membership/protocol decision changes.

Default and `formation-fault-test` worker all-target suites passed 105 tests
each (90 library, 10 binary, 5 integration). Default scoped Clippy with warnings
denied, scoped format check, `make docs-check` (89 Markdown files) and
`git diff --check` passed. Temporary diagnostic tags were removed from source
and harness. Full-workspace and observability checks were not run. Earlier
intermittent three-member convergence failures are not explained by this fix
and remain open; do not turn a later passing journey into a claimed fix for them.

After rebuilding fault artifacts with `cargo build --locked --offline
-p orishu-worker -p orishuctl --features orishu-worker/formation-fault-test
--target-dir target/formation-faults`, the original full command passed:
`python3 scripts/check-formation-cli.py --worker
target/formation-faults/debug/orishu-worker --ctl
target/formation-faults/debug/orishuctl --lost-join-ack`. This includes the
original-assignment fault marker, public handoff/churn assertions and clean
termination of all workers. The existing `proc-macro-error2` build warning
remains. No observation deadline was increased.

#### Issuer loss after insertion — process stop path verified 2026-09-06

The development-only `--test-crash-after-join` fault exits the original issuer
with code 86 inside the serialized owner turn, after insertion and accepted
ledger commit but before returning the ACK. It shares the fault feature's
normal-build exclusion, release guard and local/private startup restrictions,
and conflicts with the recoverable ACK-loss switch. Its marker contains only
the original assigned identity and proves the insertion boundary was reached.

After rebuilding isolated fault artifacts, this command passed:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl --issuer-loss
```

`make test-formation-issuer-loss` reproduces the build and command. Three real
workers and public CLI/Unix/QUIC paths are used. The source exhausts its real
default retry windows (183 seconds total; 210-second harness cutoff with
one-second status polling), with no clock hook or shortened budget. Assertions
verify retained `unresolved`, unchanged recovery reference, unchanged source
formation/node and one-member state, exact request replay without a new dial,
and refusal of a new join ID and voluntary leave. Original issuer inspection
fails while it is offline. Restarting only that dead issuer produces
`wrongIssuer` for the old reference; the source remains unchanged and is never
restarted or sent to another introducer. Surviving/restarted workers shut down
cleanly with bounded cleanup. This verifies the runbook's mandatory stop path,
not durable recovery, a cluster-wide non-insertion proof or retired-assignment
clearance.

Default and fault-enabled worker all-target suites each passed 106 tests
(90 library, 11 binary, 5 integration), including normal-build rejection and
fault-switch mutual exclusion. Fault-enabled all-target Clippy with warnings
denied, scoped formatting, `make docs-check` (89 Markdown files) and
`git diff --check` passed. The existing `proc-macro-error2` build warning remains.
The full recoverable lost-ACK/churn journey, workspace-wide checks and optional
observability matrix were not rerun in this increment; earlier convergence
failures remain open.

#### Lost suspicion notification — regression verified 2026-09-06

The convergence failure was reproduced with all three expected identities
present: A held B as `Suspected(0)`, while B and C held B as `Alive(0)`.
Fault-build instrumentation showed repeated direct contact from the locally
suspected member. An old-incarnation alive claim correctly cannot clear a
suspicion, but once the original notification/gossip was lost, later contact
did not remind the subject to refute. A shortened `--handoff-only` process
loop reproduced this on its fourth run before the fix.

The core now sends one existing `Announce(Suspect)` directly to a subject when
its authenticated Ping or correlated direct ACK leaves it locally suspected.
This does not clear suspicion, extend its timer, change incarnation ordering,
or resurrect Dead membership. It introduces no timer, queue, IO dependency or
wire/profile change; at most one additional datagram is emitted per validated
direct contact. The peer protocol records the behavior and bounds.

`direct_contact_reissues_a_lost_suspicion_without_clearing_or_extending_it`
failed before the fix and passes afterward for both Ping and ACK. It drops the
original notification, retains the original suspicion timer, delivers the
reminder to a subject core, and adopts only that subject's newer refutation.
`suspicion_reminders_do_not_follow_unknown_acks_dead_records_or_newer_refutations`
covers the negative boundaries. Both are in the membership `swim` integration
suite. Ten consecutive independent-process diagnostic journeys then passed
with the original ten-second convergence deadline:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl \
  --lost-join-ack --handoff-only
```

`--handoff-only` intentionally omits later leave/crash churn and is not the
full acceptance harness. The core all-target suite passed 205 tests plus
benchmark smoke cases, including dependency purity; its doctest passed.
The default worker suite passed 106 tests. Default scoped core/worker Clippy
with warnings denied, scoped formatting, `make docs-check` (89 Markdown files)
and `git diff --check` passed. Temporary diagnostic tags were removed.
Full-workspace and operational observability acceptance were not run.

Follow-up validation recovered the full lost-ACK/handoff/leave/crash/restart
harness result: **passed**, using the command above without `--handoff-only`.
Feature-enabled validation was rerun after the earlier test handles were no
longer available: `cargo test --locked --offline -p orishu-worker --features
formation-fault-test --all-targets --quiet` passed 106 tests, and
`cargo clippy --locked --offline -p orishu-membership -p orishu-worker --features
orishu-worker/formation-fault-test --all-targets -- -D warnings` passed.
This closes that reproduced regression, not the remaining conformance matrix.

#### Source history loss after issuer loss — verified 2026-09-06

`make test-formation-issuer-loss` **passed** after extending the independent-
process journey. It first preserves the existing real retry-exhaustion and
issuer-restart stop assertions, then deliberately kills and restarts the
unresolved source with its same private directory. Public CLI checks prove
retained certificate/operator credentials, fresh standalone formation/node
identities, exactly one live local member, and authenticated `UnknownOperation`
for the old request. The saved reference still yields `wrongIssuer`; inspection
does not change the restarted source. No new join is submitted. The injected
source crash is a conformance fault, not an operator recovery instruction.

Boundary limitation: both original process histories are lost in this case.
At this checkpoint, source loss with a surviving original issuer/retained
assignment remained open; the following checkpoint addresses it. Interrupted
retired/excluded assignment journeys and the rest of the failure matrix remain
open. No durable recovery or new admission authority is introduced.

Current-worktree validation also **passed**:

- `cargo fmt --all -- --check`;
- `cargo clippy --locked --offline --workspace --all-targets -- -D warnings`;
- `cargo test --locked --offline --workspace --all-targets --quiet`;
- `cargo test --locked --offline --workspace --doc --quiet` (one existing
  ignored example in `crates/orishu/src/model/mod.rs`, not verified);
- `python3 scripts/test_formation_evidence.py` (six tests);
- `make docs-check` and `git diff --check`.

Cargo reported the existing `proc-macro-error2 v2.0.1` future-incompatibility
warning. Optional observability acceptance was not run and remains incomplete.

#### Source loss with retained issuer history — verified 2026-09-06

`make test-formation-source-loss` **passed**. The harness's `--source-loss`
scenario uses the existing development-only lost-ACK hook after real insertion,
then briefly pauses the issuer with Unix `SIGSTOP`. It requires public source
status to remain `admitting` with a retained recovery reference; missing that
pre-adoption window fails rather than accepting an ordinary completed join.
It kills/restarts only the source with retained private files and resumes the
same issuer with `SIGCONT`. Failure cleanup kills and reaps paused workers.

Public authenticated CLI assertions establish missing source history
(`UnknownOperation`, not an arbitrary transport/authentication failure), fresh
standalone formation/node IDs, retained certificate/operator credentials and
one local live member. The original issuer must still report the original
assigned ID against its unchanged formation/node identity and the applicant
fingerprint, with no extra member allocated. Inspection does not alter the
restarted source, and no new join is submitted as recovery.

The initial direct run and five consecutive repetitions **passed**:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl --source-loss
```

This is an independent-process missing-history/stop journey, not a durable
recovery mechanism. The retained assignment may be current or liveness-retired
when inspected; either requires stopping after source history loss. Deliberate
Dead/removed/certificate-restricted replay while the original source survives
is still a separate outstanding case. No runtime, wire/profile or production
fault-control change was needed. Worker manual, runbook and administrator
story describe the tested boundary and do not prescribe the injected restart.

The full `--lost-join-ack` handoff/leave/crash/restart journey also **passed**
after the harness change. `python3 scripts/test_formation_evidence.py` passed
six tests; `make docs-check` (89 Markdown files) and `git diff --check` passed.
Workspace Rust validation from the preceding checkpoint was not rerun for
this Python/Makefile/documentation-only increment. The fault-build Make target
reported the existing `proc-macro-error2 v2.0.1` future-incompatibility warning.

#### Dead assignment with surviving source — verified 2026-09-06

The independent-process `--dead-assignment` journey **passed**:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl --dead-assignment
```

After real insertion/ledger commit and lost acceptance, the harness requires
public source status to remain `admitting`. It pauses that same source with
Unix `SIGSTOP`, lets the surviving issuer's normal SWIM timers mark the original
assignment Dead (60-second observation cutoff), and resumes the source with
`SIGCONT`. No private membership edit, fake clock, removal command or new
production fault hook establishes retirement. The original retry budget is
unchanged; source exhaustion has a 210-second observation cutoff after resume.

During recovery, public polling checks the retained reference, pending or
unresolved phase, exact issuer member-ID set and Dead liveness. At exhaustion,
the source retains its original standalone identities with `joinUnresolved`
participation; exact operator request replay returns the same record, while
new join and leave requests refuse. The issuer reports `retiredOrRestricted`
for the original ID. Inspection does not change source state. Both original
processes survive until bounded graceful cleanup.

The paired real-wire test
`peer::registry::tests::real_join_packet_drives_owner_lock_token_and_admission_outcomes`
also **passed**, using
`cargo test --locked --offline -p orishu-worker --lib real_join_packet_drives_owner -- --nocapture`.
It directly exercises replay rejection on real reconnect/JoinReq streams;
the process journey establishes the operator-visible recovery/stop boundary,
not an exact count of those wire rejections.

This closes the liveness-Dead combination only. Removed/tombstoned and
certificate-blocked assignments, excluded restart/ejection and the other
formation failure rows remain distinct requirements. The worker manual,
recovery runbook and administrator story reflect that limit. There is no
runtime, wire/profile or authority change in this increment.

The second isolated run through `make test-formation-dead-assignment` also
**passed**, as did the shared-setup regression `make test-formation-source-loss`.
`python3 scripts/test_formation_evidence.py` passed six tests; `make docs-check`
(89 Markdown files) and `git diff --check` passed. Full workspace Rust and
combined observability acceptance were not rerun for this harness/docs change.
Fault builds still report the existing `proc-macro-error2 v2.0.1`
future-incompatibility warning.

#### Removed assignment with surviving source — verified 2026-09-06

The independent-process `--removed-assignment` journey **passed**:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl \
  --features orishu-worker/formation-fault-test --target-dir target/formation-faults
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl --removed-assignment
```

`make test-formation-removed-assignment` provides the equivalent build/run
shortcut. The development-only `--test-remove-after-join` startup hook discards
acceptance after real insertion and ledger commit, then submits the existing
force-removal command through the serialized owner before publishing its
secret-free assigned-ID marker. It never edits the membership map directly.
The hook is one-shot, shares the loopback/Unix/private-directory guards and
cannot be combined with the other startup faults. Ordinary builds reject it.

Public process checks prove the original pending operation retains its
reference and identities through bounded retry exhaustion, while issuer
inspection reports `retiredOrRestricted` and its exact member set contains
neither the removed assignment nor a replacement. Exact operator replay
returns the same exhausted record; new join/leave requests refuse. Both
original processes survive until graceful cleanup. This is a test-arranged
removal, not an administrative API or a suggested operator recovery action.

Default and `formation-fault-test` worker all-target suites each **passed 107
tests**, including switch availability/conflict checks. Scoped worker Clippy
with warnings denied passed both configurations; `cargo fmt --all -- --check`,
six evidence-helper tests, `make docs-check` (89 Markdown files) and
`git diff --check` passed. No wire/profile, dependency or production authority
change was introduced. Certificate-blocked recovery, excluded restart/ejection,
other formation fault rows and observability remain open. Full-workspace
acceptance was not rerun in this increment.

The full process harness with `--lost-join-ack` also **passed** after the fault
selector changed, preserving original-assignment recovery, three-worker
handoff, leave, crash/restart and readmission. Fault-build Cargo commands still
report the existing `proc-macro-error2 v2.0.1` future-incompatibility warning.

#### Certificate-blocked assignment with surviving source — verified 2026-09-06

`make test-formation-blocked-assignment` **passed**. It builds the isolated
development fault worker and runs the public `--blocked-assignment` journey.
The one-shot `--test-block-after-join` startup hook applies the existing
`UpdateBlocklist` command to the authenticated applicant fingerprint after
real insertion/ledger commit, before delivering acceptance. It uses the
serialized owner, not a private membership-map edit, and publishes the
secret-free assigned-ID marker after the transition. The switch is absent
from ordinary builds, mutually exclusive with all other startup faults, and
subject to the same loopback/Unix/explicit-private-directory guard.

The original source remains pre-adoption and exhausts its unchanged retry
budget within the 210-second observation cutoff. Throughout recovery, public
issuer inspection reports `retiredOrRestricted`, the member record retains
the original fingerprint, and the exact member-ID set has no replacement.
The source retains its original identity/reference, exact request replay
returns the same exhausted record, and new join/leave work refuses. Both
original processes survive until bounded graceful cleanup. No certificate
rotation, exclusion removal, public blocklist API or new recovery authority
is introduced.

Default and fault-enabled worker all-target suites each **passed 108 tests**.
Scoped worker Clippy with warnings denied passed both configurations;
workspace formatting, six evidence-helper tests, `make docs-check` (89 Markdown
files) and `git diff --check` passed. An initial compile check exposed an
incorrect blocklist type import path, corrected before these passing checks.
No full-workspace or observability acceptance was rerun in this increment.
The existing `proc-macro-error2 v2.0.1` future-incompatibility warning remains.

This proves certificate-blocked interrupted recovery with the original source
alive. Excluded restart followed by fresh admission, post-adoption ejection,
remaining unavailable-history combinations and the broader fault matrix still
require their own evidence. The runbook and operator story preserve these
limits; passing this case alone does not close the recovery slice or M4.

The full `--lost-join-ack` independent-process handoff/leave/crash/restart
journey also **passed** against the rebuilt fault worker after this change.

#### Excluded restart and fresh admission — verified 2026-09-06

`make test-formation-excluded-restart` **passed**, including a successful
different-certificate control. The earlier direct `--excluded-restart` run
also passed before that control was added; it is narrower evidence, not a
second run of the complete final scenario.

The journey uses the existing certificate-block-before-ACK hook, requires
the original source to remain pre-adoption, then kills/restarts only that
source with its private directory retained. Public CLI checks establish the
same certificate/operator credentials, fresh standalone formation/node IDs,
one local member and `UnknownOperation` for the original request. The same
surviving issuer still reports the original assignment `retiredOrRestricted`.

As a deliberate negative test—not operator recovery—the harness submits a
fresh operation with current standalone preconditions and original issuer
material. It checks that a new peer-attempt ID uses the same blocked
fingerprint, never adopts or allocates a replacement, and reaches `unresolved`
within the unchanged 210-second observation cutoff. Exact replay cannot reset
that terminal record. The original issuer reports `recordUnavailable` for
the new attempt and retains the old restricted assignment. Missing history
still does not establish general permission to admit a worker.

A third worker with a different certificate then completes admission/catch-up
through the same issuer and join material. Its distinct assigned ID is the
only new member, and the excluded source remains unchanged. This rules out a
dead listener or blanket formation lock as the explanation for refusal.
All fault setup and assertions use independent processes and production
interfaces; no runtime, wire/profile or new fault-control change was needed.

This closes the certificate-excluded restart/fresh-attempt combination, not
post-adoption self-ejection or the entire combined restart/ejection matrix row.
The worker manual, runbook and administrator story preserve the distinction
between a negative bypass test and a safe operator procedure.

`make test-formation-source-loss` also **passed** after the shared restart
setup changed. Six evidence-helper tests, `make docs-check` (89 Markdown files)
and `git diff --check` passed. Workspace Rust tests, full churn and optional
observability acceptance were not rerun for this Python/Makefile/docs-only
increment. The existing fault-build `proc-macro-error2 v2.0.1`
future-incompatibility warning remains.

#### Post-adoption peer-wire self-ejection — verified 2026-09-06

`make test-formation-peer-ejection` **passed** after the HTTP harness regression
described below was fixed. The public journey first verifies completed
adoption/catch-up and current `joined` participation. A guarded development-
only Unix signal fixture on the original introducer then sends one valid
tombstone-bearing Ping datagram through the existing codec and authenticated
member connection to its sole live peer. It rejects absent/ambiguous routes
and deferred gossip, with no retry, extra queue or reliable-stream fallback.

The receiver must report `ejected`, preserve its formation/node IDs, withhold
join material and introduction, and keep its historical joined receipt. The
fixture sender's surviving view eventually marks it Dead, establishing that
the receiver no longer answers live membership probes. The receiver remains
ejected until the harness explicitly leaves using its current formation
precondition, yielding fresh standalone IDs and one live local member. Output
must not contain the old formation token. All observations use the public CLI.

The fixture sends a valid test-peer tombstone without mutating the sender's
own model. As allowed by the failure matrix, this proves receiving-worker
self-ejection, not a new public removal API or cluster-wide removal convergence.
The startup-only `--test-eject-peer-on-signal` switch registers a one-shot
`SIGUSR1` handler only in the explicit fault build, requires the existing
loopback/Unix/private-directory safeguards and conflicts with other startup
faults. The ordinary CLI rejects it; non-Unix invocation is unsupported.

The initial direct journey passed. A later Make run failed **before ejection**
with `BrokenPipeError` in a malformed-inspection check; private evidence is
retained at `/tmp/orishu-formation-failure-91svg76i`. The test client sent headers
and body separately, but the server could reject the headers and close its
read side before the body write. A real-socket, deterministic regression at
the same helper reproduced the failure before the fix. The helper now reads
the response after that specific write error, without retrying the request or
treating transport failure as acceptance. Exact expected status and response
size remain mandatory. Four `scripts/test_formation_http.py` tests pass for
the early response, wrong status, no response and oversized response. The
original Make journey was rerun and passed; no temporary debug logging remains.

Default and fault-enabled worker all-target suites each **passed 109 tests**;
scoped worker Clippy passed both feature configurations. Workspace formatting,
six evidence-helper tests, four HTTP-helper tests, `make docs-check` (89 Markdown
files) and `git diff --check` passed. The full lost-ACK/handoff/leave/crash/restart
journey also passed after adding the owner fixture, before the HTTP helper fix.
Workspace-wide Rust tests and observability acceptance were not rerun; the
existing `proc-macro-error2 v2.0.1` future-incompatibility warning remains.

#### Lost voluntary departure announcements — verified 2026-09-06

The full independent-process `--lost-departure` journey **passed**:

```sh
python3 scripts/check-formation-cli.py \
  --worker target/formation-faults/debug/orishu-worker \
  --ctl target/formation-faults/debug/orishuctl --lost-departure
```

The one-shot, development-only `--test-drop-next-departure` startup fault is
armed on C. On its voluntary leave, the owner encodes the normal departure
effects under the old formation/assigned identity but suppresses delivery at
the existing datagram-send boundary. The public harness requires the exact
old assigned-ID marker and a count of two suppressed announcements. The core
leave transition, receipt, fresh standalone identity, session closure and
generation fencing are unchanged. The flag is absent from normal builds,
conflicts with all other startup faults and uses the existing exposure guards.

Survivors must still report the departed ID Dead through normal SWIM within
the unchanged 60-second observation cutoff. C replays its original receipt,
then rejoins with the same certificate and a fresh assigned ID while old dead
history remains. No tombstone/block is cleared to make readmission succeed.
The full journey continues through ordinary crash, restart and readmission,
exact member/fingerprint/liveness assertions and bounded graceful cleanup.

This closes deliberate announcement loss, not loss of the public leave
response: interrupted operator-request/receipt recovery remains a separate
case. It does not claim a production convergence SLO or prove arbitrary churn.
The worker manual documents the fault boundary and the normal SWIM fallback.

Default and fault-enabled worker all-target suites each **passed 110 tests**,
including startup-switch availability and mutual exclusion. Feature-enabled
scoped Clippy, workspace formatting, six evidence-helper tests, four HTTP-helper
tests and `git diff --check` passed. Full-workspace Rust and observability
acceptance were not rerun in this increment. The existing fault-build
`proc-macro-error2 v2.0.1` future-incompatibility warning remains.

The second isolated run through `make test-formation-lost-departure` also
**passed**, as did default scoped worker Clippy with warnings denied,
`make docs-check` (89 Markdown files) and the final `git diff --check`.

#### Lost public leave response — verified 2026-09-06

The full ordinary-build `--lost-leave-response` process journey **passed**:

```sh
cargo build --locked --offline -p orishu-worker -p orishuctl
python3 scripts/check-formation-cli.py --lost-leave-response
```

A single-request Unix-socket proxy in the private harness directory forwards
the real CLI's authenticated leave request unchanged to C. It requires HTTP
200 from the worker, then closes both sockets without forwarding response
bytes. The CLI must report failure. Independent public inspection must show
C already standalone with fresh identities before the exact request is retried
directly. The returned receipt must identify that same completed transition;
current formation/node identity must remain unchanged by replay.

The rest of the normal journey remains enabled: stale preconditions and changed
payload reject, exact receipts replay, survivors detect departure, the same
certificate rejoins with a fresh ID, and replay after readmission cannot leave
again. Ordinary crash/restart/readmission and bounded cleanup also pass.

The proxy bounds headers to 8 KiB, request body to 4 KiB, one connection and
finite socket/deadline waits. Raw authenticated bytes remain in memory and
never enter failure artifacts. Two real-socket proxy tests verify unchanged
forwarding with zero response bytes delivered and refusal to count HTTP 401
as lost acceptance. Together with the four early-rejection regressions,
`python3 scripts/test_formation_http.py` **passed six tests**. This introduces
no worker hook, protocol change or new recovery authority. It proves recovery
while the original process retains the receipt, not durable restart recovery.

Six evidence-helper tests, `make docs-check` (89 Markdown files) and
`git diff --check` passed. The worker/CLI build reported the existing
`proc-macro-error2 v2.0.1` future-incompatibility warning. Full-workspace Rust
and observability acceptance were not rerun for this harness/docs-only change.

The second isolated run through `make test-formation-lost-leave-response`
also **passed**, including its HTTP-helper suite and full process journey.
