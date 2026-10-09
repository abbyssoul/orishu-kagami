# ADR 0032: Retain identified run-command outcomes before execution

Status: **implementation refinement; shared facts, Unix journal, daemon coordination, opt-in HTTP controls, shared client and Kagami headless controls implemented; window controls remain gated**
Date: **2026-09-17**
Refines: [ADR 0028](0028-fence-worker-scientific-admission-through-formation-owner.md)
and [ADR 0030](0030-retain-durable-worker-load-receipts.md).

## Problem and options

The retained executor can advance and terminally finish a run. A request can lose
its response after publication. Exact-boundary guards prevent a retry from making
an additional step, but a changed boundary does not identify which caller caused
it. Returning current status as if it were that command's receipt invents history.

- **Unconditional retry or refresh the expected boundary.** Rejected: this can
  silently advance scientific time again.
- **Boundary comparison alone.** Required at the executor, insufficient for
  attributing a particular operation or recovering historical outcomes.
- **Only retain volatile receipts.** Insufficient for the selected recovery
  semantics: process restart must not erase whether an operation may have applied.
- **Atomically persist receipts and all scientific state.** Deferred to durable
  scientific commit; a metadata journal must not pretend to provide it.
- **Durable intent and conservative final facts, sharing the load journal IO
  machinery.** Selected. This reuses the established failure/ownership rules
  without copying filesystem logic or changing existing load files.

## Decision

Use the shared [run-command facts](../run-command-receipts-v1.md). A request binds
one operation ID, the complete owner-issued execution identity, an explicit
expected boundary and one action. The first manual-execution profile supports
`step` (exactly one fixed step) and `finish` (terminal integration shutdown).
**Finish is not resumable pause.** Continuous run, step budgets, pause/resume,
unload and reset require their own reviewed lifecycle integration; do not map
their names onto a terminal stop merely because an internal method exists.

Command replay keys are `(formationId, operationId)` in a separate command
namespace. Rebinding root, epoch, boundary or action conflicts. A completion
ticket is issued only after Pending intent is durable. The daemon must own that
ticket independently of HTTP/client futures, resolve history before checking a
currently usable run, and invoke the executor's guarded operation against the
exact identity. Only known owner acknowledgement permits Applied. A known
non-application permits Refused; ambiguous publication/reply loss requires
Indeterminate. Restart converts Pending to Indeterminate before opening history.
These integration obligations are supplied by the explicitly installed internal
daemon coordinator, not the journal itself. One detached command owns the ticket
through final IO; client response loss does not cancel it. Daemon shutdown/drop
revokes new commands and cancels uncommitted work, while known committed outcomes
still finish their receipt IO. A poisoned journal does not discard the live run;
exact committed status remains readable independently of historical attribution.

Applied receipts contain validated committed metadata with the exact descriptor,
resulting boundary, finite nonnegative SI simulation time and phase. Step must
produce expected boundary + 1 and Ready; finish must retain the expected boundary
and produce Finished. A receipt is historical, not proof of current availability,
restored scientific bytes or distributed consensus. No cache/path/plugin lookup
can replace the pinned run identity.

The two sealed journal profiles use one private-directory, single-writer,
bounded-reader, staged-write/sync/rename/directory-sync implementation. The load
profile retains its exact v1 schema and filenames. The command profile uses
separate fixed names and version; neither profile reads or rewrites the other.
No on-disk migration or new runtime dependency is required. Record policies are
sealed and cannot be supplied by plugins or network input.

Committed simulation time requires CBOR floating-point metadata. Only the
command journal and command/status response profiles opt into finite floats; peer encode/decode and load
records keep their existing float prohibition. All profiles retain the same
byte, structure, duplicate-field and allocation preflight bounds. NaN and
infinity are rejected at both preflight and shared typed construction.

### Public manual-control refinement

The [scientific-command HTTP profile](../protocol-scientific-command-v1.md) now
composes this coordinator with opt-in startup and authenticated bounded routes.
It shares scientific load's operator/TLS authority and handler capacity rather
than opening a separate unauthenticated surface. Startup opens both journals
before listening; incompatible history prevents enabled startup. Complete small
CBOR bodies precede dispatch, with no workload/artifact body or plugin lookup.

Status uses a versioned POST body carrying the exact run identity. This keeps
identity in the same bounded typed decoder instead of a new query-string parser;
the operation remains read-only. Current-run discovery alone cannot select a
mutation target. The shared client correlates complete receipt intent and exact
status identity and does not retry or substitute a current boundary. Final
receipts use HTTP 200 regardless of domain outcome; Pending submission uses 202.

## Consequences and delivery gates

Each profile currently retains at most 256 records in a 512 KiB snapshot. Full
history refuses new commands but retains lookup/replay/conflict handling. Never
evict to make an old identity executable. Command history exhaustion is independent
of load history. Retention/acknowledgement/tombstone policy and scalable persistence
remain explicit gates before sustained interactive execution; this is a bounded
manual-control foundation, not the final high-frequency scheduler design.

Snapshot writes/copies are O(retained records + encoded bytes), with sync IO on
an imperative lane, never the formation owner or numerical guest. This cost must
not be implicitly added per tick to a future continuous/step-budget loop. Disk
errors poison the journal until reopen, including failures after rename. Durable
Pending prevents fresh execution permission even if the final write was lost.
The journal is Unix-only, depends on filesystem sync semantics, and does not
protect against an administrator rolling back its storage.

Daemon ownership, exact executor/status projection, conservative error mapping
and real-kernel cancellation/publication/reply-loss tests are implemented. Opt-in
startup, authenticated bounded serving and shared-client correlation now have
real-worker step/finish/replay/restart evidence. Kagami's headless controls now
consume the shared methods without introducing retry or a second run authority.
Next gates: window controls, observations and the wider lifecycle/retention work above. Internal
coordinator installation alone still does not enable a route.
