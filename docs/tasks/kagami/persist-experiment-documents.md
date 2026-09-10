# Persist and recover experiment documents

Status: **implemented**; slices 1–5 landed across
`kagami_session::{document, store}`, `kagami_document::hydrate` and
`DocumentAuthority`. ADR 0022's `default_view` section landed with
[K11](implement-kagami-viewport-workflows.md) slice 2, which advanced the
format to version 2  
Work package: **K-DOCUMENT** ([roadmap](../../roadmap/README.md))  
Decisions: [ADR 0012](../../adr/0012-start-with-file-sharing-and-preserve-collaborative-authoring.md),
[ADR 0019](../../adr/0019-kagami-experiment-document-model.md),
[ADR 0005](../../adr/0005-author-numeric-values-as-unit-aware-expressions.md),
[ADR 0022](../../adr/0022-persist-default-view-outside-experiment-intent.md)
Stories: [Save an experiment to a file](../../user-stories/kagami/authoring.md),
[Open a previously saved experiment](../../user-stories/kagami/authoring.md),
[Share an experiment draft](../../user-stories/kagami/authoring.md)

## Outcome

An experiment round-trips through a versioned file. A researcher saves a
draft, sends the file to a colleague, and the colleague opens it and sees the
same experiment — including the authored expressions, not just the numbers
they happened to resolve to. This is ADR 0012's entire first collaboration
model, so the format is the sharing protocol.

An interrupted or corrupted write never costs the previous document.

## Owning boundary

Owns the `kagami.experiment` format, its version policy, the pure codec, and
the durable write protocol. The versioned DTO and codec may live in
`kagami-session`; filesystem operations are an imperative-shell module used by
the app and MCP adapters. The `DocumentAuthority` itself remains a decision
surface over values and IO completion messages: it never opens a path, reads a
clock, or treats a successful syscall as command acceptance.

## Source assessment

`../field-cad/crates/fieldcad-scene-document/src/lib.rs` is a proven
implementation and the mechanics transfer almost directly:

- A `FORMAT_ID` checked before any other field is interpreted, and a
  `FORMAT_VERSION` where a *higher* version is refused outright rather than
  partially interpreted. Field CAD reached version 9 by adding fields; the
  policy that made that safe is the one to copy.
- Explicit per-version persisted DTOs and conversion through domain
  constructors. `#[serde(default)]` is used only for a field whose absence has
  defined semantics in that version; it is not a substitute for a version
  policy. Unknown fields in a supported version are refused so a producer that
  forgot to advance the version cannot have its content silently discarded.
- The write protocol: create a sibling temporary file without following an
  existing link, write and validate the complete bytes, `sync_all`, and — only
  if the existing primary independently parses as a valid document — replace
  the backup with that verified primary before atomically replacing the
  primary. Sync the containing directory where the platform supports the
  durability operation. Retain one backup of the previous document unless its
  bytes are damaged, rather than of whatever happened to be on disk — Field CAD
  spelled this as "verified", which turned out to be the wrong predicate.
- The load fallback: primary, then `.bak`, then the bounded recoverable sibling
  temporary candidate(s), each independently decoded and version-checked,
  reporting *which* candidate was used so the caller can warn. The fallback is
  entered only when the primary is *damaged* — an intact primary this build
  declines stops the load there, because recovering around it would end in
  overwriting it.
- Deliberate exclusions that transfer: no session identity (it names a live
  process, not a saved artifact) and no run/paused mode (opening a file must
  never immediately consume machine resources).

What does **not** transfer wholesale: Field CAD's `SceneViewState`, probe/distance/
mass-aggregate histories, `run_records`, recording state and `quick_add_hidden`
were all in its scene document. Under ADR 0012 those are presentation state or
run observations, and under ADR 0004 they are not authored intent. ADR 0022
does require a narrow, separately versioned default-view section in the file,
preserving projection, camera pose and orbit/focus; histories and run records
remain excluded.

## Implementation slices

### 1. The versioned envelope and evolution policy — **implemented**, including
ADR 0022's `default_view` section, which arrived with K11 slice 2 as format
version 2

- `format: "kagami.experiment"` and a numeric `format_version`, checked in
  that order before any content is interpreted. Do not encode version twice in
  both fields.
- Metadata: generator identity and version, creation timestamp preserved
  across re-saves, and a save timestamp. The generator string is for support,
  not parsing.
- Each supported version gets an explicit DTO and conversion; versions newer
  than this build are rejected. There is no imaginary version 0 and no generic
  "lower is safe" rule. Version 2 is the first exercise of that ladder: version
  1 loads through `DocumentV1` and converts *up*, so a re-save writes version 2,
  and the version-1 golden fixtures are retained as the conversion's regression
  input.
- Define whether unknown fields are rejected and which fields may be absent in
  each supported version. A format change that would otherwise be silently
  lost on re-save advances `format_version`.
- Encoding is JSON, matching the repository's persisted-data convention; the
  hand-authored YAML format stays the catalog's alone.
- The envelope has a separately versioned optional `default_view` owned by the
  client. Its version 1 preserves projection mode, camera pose and orbit/focus;
  this section is decoded with
  presentation bounds and is excluded from experiment revision and workload
  compilation (ADR 0022). Its version policy is the *inverse* of the
  envelope's: a section this build cannot read is reported and dropped while
  the experiment opens normally, because presentation must never be why a file
  fails to open.

### 2. The pure codec — **implemented** for both sections

- A versioned persisted DTO separates the public file contract from
  `Experiment`'s private `Arc` layout. `encode` receives an immutable
  experiment snapshot, an immutable authoring-view snapshot and metadata
  supplied by the shell; `decode` produces both persisted sections, with the
  experiment candidate reconstructed through domain constructors and each
  section checked against its own structural bounds.
- The codec performs no IO and reads no clock, so round-trip properties are
  testable as values. Timestamp and generator metadata are explicit inputs.
- The encoded form preserves: object identities and identity counters,
  component type identities and the schema versions their values were checked
  against, authored expression source, variable definitions with their stable
  identities and namespaces, declared dimensions, setup, and the enabled plugin
  composition.
- Resolved values are **not** persisted. Decoding recompiles and evaluates
  authored expressions when their variables, catalog projection, and
  compatible schemas are available, and otherwise retains the source with a
  structured unavailable diagnostic; a file cannot inject a cached SI value
  into the authority.

### 3. Absent plugins and forward compatibility — **implemented**

- A document naming a component type this installation does not have loads
  successfully and reports that component as *unavailable*, preserving all
  authored fields and expression source semantically. Installing the plugin
  later makes it available with no edit; removing a plugin never mutates or
  discards experiment data. JSON whitespace and object-member order are not
  authored intent and need not survive re-encoding.
  This is Milestone 7's exit criterion, and it is decided by the format.
- An unavailable component blocks compilation and validation of edits that
  touch it, and is reported as a structured diagnostic — not as a load failure
  and never with a fabricated default.
- Golden fixtures: a minimal document, a document with expressions and
  variables, a document with an unavailable component, a document one version
  ahead, and hostile inputs (truncated, oversized, wrong format id, wrong
  types, deep nesting).

### 4. The durable write and recovery shell — **implemented**

- Implement the sibling-temp/fsync/verified-backup/atomic-replace protocol and
  the primary/backup/temp load fallback described above, confined to one
  imperative-shell module. State and test the guarantees supported on each
  target platform; do not call a rename atomic where replacement semantics do
  not provide that guarantee.
- Report which candidate a load actually used, so anything other than the
  primary surfaces as a warning rather than passing silently.
- Bound the accepted file size before reading it into memory.
- Inject filesystem operations behind a narrow test seam and exercise failures
  after temporary creation, write, file sync, backup replacement, primary
  replacement, and directory sync. A fixed `.tmp` name may be recovered only
  if stale-file and link behavior are explicitly bounded; otherwise use a
  bounded set of unique sibling candidates.

### 5. Document lifecycle through the authority — **implemented**

- `new` and replacing the current experiment with a decoded candidate are
  attributed authority operations. File selection, reads, writes, and clocks
  are shell effects; their typed outcomes return to the authority.
- `save` captures one exact experiment revision and one exact authoring-view
  revision. Only a successful completion naming both may update the file clean
  marker; if either advanced while IO was in flight, the file stays dirty. Save
  As adopts its new target only on that same successful completion. UI and MCP
  save the current authoring view but neither may mutate it as part of saving.
- Opening validates and adopts one decoded candidate atomically, then clears
  history and establishes the loaded revision as clean. Define revision
  rebasing explicitly: persisted revision numbers are provenance in the file,
  not permission to rewind a live session's monotonic revision or event
  sequence.
- Where an interactive user would be asked a question — replacing an experiment
  with unsaved changes — an MCP caller supplies that decision explicitly as a
  request field (ADR 0006). The authority never resolves it silently.

## Implementation record for slices 1–3

`kagami_session::document` owns the `kagami.experiment` format and its codec;
`kagami_document::hydrate` owns the one validated ingress from a decoded
document into an `Experiment`. 14 acceptance tests in
`crates/kagami-session/tests/document.rs`, with golden fixtures.

Four decisions are worth carrying forward:

- **A quantity can be stored unpriced.** Loading a document whose component
  schema is absent cannot produce a magnitude or a dimension — both come from
  a declaration this machine does not have — so `PropertyValue::Unresolved`
  retains the expression and admits it has no number. Inventing a default
  would have been the one thing slice 3 forbids. Installing the plugin later
  reports `value_not_priced`, and the next edit that *touches the component*
  prices it, which makes an ordinary edit the repair rather than a migration.
- **The saved revision is metadata, not experiment content.** It was
  originally inside the experiment section, and the byte-identity test caught
  the consequence: opening establishes a fresh history, so a re-save changed
  that field and every shared file churned. It now sits beside the generator
  and the timestamps, as provenance about the *save*, and the authored section
  re-encodes byte-identically.
- **Counters are the identity parse boundary for a file.** A document may name
  an identity only if the counters it also carries say that identity was
  allocated. Otherwise a file could claim an object the session would later
  mint, and two different objects would end up sharing a handle.
- **Unknown fields are refused.** A producer that added a field without
  advancing the version would otherwise have it silently dropped on the next
  re-save — the failure a send-someone-a-file workflow can least afford.

Slice 5 followed, adding `SessionCommand::New` and `SessionCommand::Open` as
attributed submissions. Three decisions there:

- **Opening rebases forward.** A decoded document arrives at the initial
  revision, and is adopted onto the running session's *next* one through
  `Experiment::adopted_after`. A view or cache that had already caught up to
  r47 must never be told it is now at r3, so the revision a file recorded
  stays provenance about the session that saved it.
- **Discarding unsaved work is always the caller's decision.** A replacement
  while dirty is refused with `unsaved_changes` unless the request says to
  discard. Where a UI shows a dialog, an MCP caller states the answer
  (ADR 0006); the authority resolves it in neither direction.
- **`Open` has no wire form.** It carries a whole decoded experiment rather
  than request fields, so `WireEnvelope::of` returns `None` for it. An adapter
  names the *file* to the shell, which reads and decodes it and submits the
  candidate — encoding it as anything else would quietly change the request.

Slice 4 followed as `kagami_session::store`, behind a `FileStore` seam so
every step's failure is reachable in a test — 11 of them in
`tests/store.rs`. Three decisions:

- **The backup is a copy of the last document that was not *damaged*.** Step 4
  moves the existing primary aside unless its bytes are truncated or garbled.
  Promoting damage would replace a backup that might still have been good — but
  a document that merely fails to decode *here*, such as one from a newer build,
  is intact and is preserved. This originally read "the last **verified**
  document" and moved the primary aside only if it decoded; see the K11 note
  below for why that predicate was wrong and what it cost.
- **One window is unavoidable, and it is documented rather than hidden.**
  Because the previous document is *moved* aside rather than copied, a failure
  between that move and the final rename leaves the document only in the
  backup. It survives whole, and the next open reports `LoadedFrom::Backup` so
  the user is told. A test asserts exactly this rather than claiming the
  primary is always intact — which is what the first draft of that test
  wrongly asserted.
- **Temporaries are a bounded set of unique siblings, not a fixed `.tmp`.**
  Recovering a fixed name safely means bounding stale-file and link
  behaviour; trying a few exclusively-created candidates needs no such rules.

The bookkeeping it reports into — which revision is on disk and where —
landed earlier with the [boundary follow-up](harden-document-boundaries.md) as
`DocumentAuthority::acknowledge_save`.

[K11](implement-kagami-viewport-workflows.md) slice 2 completed the
`default_view` half and advanced the format to version 2. Two of its decisions
belong to this task's record:

- **A float has to survive the file exactly.** `serde_json`'s default parser
  is fast rather than bit-exact, so an authored coordinate could come back one
  bit away from what was written — and every fixture here used
  exactly-representable values, so nothing noticed. The workspace now enables
  serde_json's `float_roundtrip` feature, and
  `a_coordinate_survives_the_file_bit_for_bit` covers positions, velocities and
  camera poses together. "Save then open reproduces an identical experiment"
  was previously only true for round numbers.
- **A refusal now says why.** `store::load` distinguishes
  `LoadError::Refused` from `LoadError::Unreadable`, so a document written by a
  newer build reports its version instead of appearing absent.
- **Recovery is for damage, and only for damage.** This is the correction that
  matters most in this module. Slice 4's protocol asked one question of the
  existing primary — "does it decode?" — and treated every no the same way. But
  truncated bytes and a document from a newer build are opposite situations:
  the first wants the backup, and the second must not be touched. Conflating
  them meant an unsupported-version primary with a readable older backup
  *silently opened the backup*, reported a successful recovery, and let the
  next save replace the intact newer document — which the backup-promotion rule
  then declined to preserve, because it did not decode. Both halves now turn on
  `DocumentError::is_damage`: `load` falls back only for damage, and `save`
  moves anything that is not damage aside as the backup first.

  This sharpens, rather than contradicts, the earlier decision that "the backup
  is a copy of the last *verified* document". Verified was the wrong predicate;
  the right one is *not damaged*. A document this build cannot read is still
  irreplaceable by anything this build could write.

## Acceptance criteria

- Save then open reproduces an identical experiment: identities, counters,
  expression source, variable definitions and namespaces, dimensions, schema
  versions, setup, and plugin composition.
- The selected perspective/orthographic default round-trips independently;
  changing it does not advance the experiment revision or enter document undo,
  and a delayed save cannot clear a newer experiment or view edit.
- Re-encoding the same authored state and authoring view with the same supplied
  metadata produces canonical byte-identical output. A normal re-save may
  change its explicit save timestamp and generator version while decoding to
  identical authored and view state.
- A document whose `format` differs, or whose `format_version` is higher than
  this build supports, is refused outright with a clear reason. Zero is refused
  too; older versions load only through their explicit conversion.
- A document naming an uninstalled component loads with that component
  preserved and reported unavailable, and never with a substituted value.
- Killing the process between the temp write and the rename leaves the previous
  document intact and recoverable; a corrupted primary falls back to the backup
  and says so.
- A document this build declines rather than fails to read — a newer format
  version, another format — is never recovered around and never overwritten
  without being preserved. Opening it refuses with its version; saving over it
  moves it to the backup first.
- A late successful write of revision N cannot mark revision N+1 clean, and a
  failed Save As changes neither the clean marker nor the current target.
- Truncated, oversized, wrong-typed and deeply nested inputs produce bounded
  typed errors and never panic.
- No presentation state except the explicit default-view envelope, run
  observation, run record, or recording appears in the format.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- The portable workload bundle: that is compilation output, immutable, and
  content-addressed
  ([ADR 0010](../../adr/0010-content-addressed-workload-closure-and-portable-bundles.md)),
  not this file.
- Named views, saved observation exports, run records, or session recordings.
- Multi-writer merge, locking, or conflict resolution (ADR 0012).
- A generic migration framework. Each older version, when one exists, gets an
  explicit bounded DTO-to-domain conversion and fixtures.
