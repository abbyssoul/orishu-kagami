# Fix Kagami catalog authority collision and path-containment boundaries

Status: **implemented and accepted in `crates/kagami-catalog`, including the
follow-up containment correction; the core acceptance blocker is cleared**

Parent task:
[Implement the Kagami object catalog](implement-kagami-object-catalog.md)

Decisions:
[ADR 0008](../adr/0008-catalog-templates-instantiate-self-contained-objects.md)
and
[ADR 0018](../adr/0018-catalog-values-are-captured-by-reference-not-linked.md)

## Outcome

Catalog create/update commands cannot introduce duplicate template identities,
renames publish enough information for cached views to converge, and every
catalog file effect remains physically contained beneath the configured catalog
root even when directories or temporary-file names contain symlinks. Rejected
commands leave files, the loaded projection, revision, event history, and
idempotency history unchanged.

This is a narrow correction to the implemented catalog authority and writer.
It does not add document, UI, MCP transport, workload-capture, or dimension
inference integration.

## What landed

- `CatalogAuthority::check_identity_available` decides identity ownership
  against the current snapshot before any file effect, for both `Create` and
  `Update`. Any entry that exposes a valid identity owns it, whatever its load
  state. A command that proposes the identity it is already replacing is not a
  rename and stays legal.
- `CatalogChange::Updated` and `CatalogOutcome::Updated` carry `previous` and
  `current`, so a cached view removes the retired row and inserts the new one.
- `write::CatalogRoot` canonicalises the configured root and then proves each
  requested component in turn rather than resolving an assembled path: a
  component that exists must be a real directory whose canonical form still
  lies beneath the root, and the walk continues from that proven directory. A
  target that is not a regular file is refused as `WriteError::NotContained`.
  The rule covers reads, creates, updates, deletes, digests, and the sibling
  temporary file.
- A missing parent directory is created one component at a time, only below a
  component already proven, and `ContainedPath::undo_created_directories`
  removes what the operation added — empty directories only, deepest first — if
  any later step fails.
- Temporary files are created with `create_new` under an unpredictable name,
  so a planted symlink or another writer's file is neither followed nor
  clobbered. Same-directory atomic replacement and cleanup are unchanged.
- The loader reports a symlinked catalog entry as
  `InvalidReason::SymbolicLink` instead of adopting bytes it does not own.

Symlink tests are `#[cfg(unix)]`; the containment mechanism itself
(`canonicalize`, `symlink_metadata`, `create_dir`, `create_new`) is `std` and
portable, so no dependency was added to the crate's budget.

The race boundary is stated in the `write` module documentation and the crate
README rather than left implied: path-based `std` operations contain a
*pre-existing* hostile link, which is what an untrusted command can plant, but
not a process concurrently swapping a directory between one check and the next.
No protection against concurrent directory replacement is claimed, because none
is implemented.

## Independent review finding: missing descendants escape before rejection

Corrected. The section below describes the defect as it was reported and the
sequence the correction implements.

The first containment correction still performed an out-of-root filesystem
effect for one case. `CatalogRoot::contained` asks `canonical_directory` to
resolve the requested parent directory with `create_parents = true`. When that
directory does not exist, `canonical_directory` calls `create_dir_all` on the
unresolved joined path and checks the canonical result only afterwards.

For example, given:

```text
<catalog-root>/linked -> <outside-directory>
requested file = linked/created-outside/planets.yaml
```

`create_entry` returns `WriteError::NotContained`, but first creates
`<outside-directory>/created-outside`. This was reproduced through the public
writer API during independent review. The command is rejected, yet it followed
an existing symlink and changed filesystem state outside the authority's root.

The current symlink-parent regression uses `sub/secret.yaml`, where the whole
parent already exists through the link. Canonicalisation therefore rejects it
before `create_dir_all`. It does not exercise a missing descendant below the
linked parent.

### Required correction

Do not call `create_dir_all` on a path assembled from the configured root and
untrusted relative components before proving where its existing prefix leads.

Use this sequence for a create whose parent may be missing:

1. Canonicalise the configured catalog root. A symlink used as the configured
   root remains an intentional, supported configuration; this exception does
   not extend to descendant components supplied by a command.
2. Find the deepest existing ancestor of the requested parent without creating
   anything. Canonicalise that ancestor and reject the operation unless it is
   the root itself or lies beneath the canonical root.
3. Reject an existing descendant component that is a symbolic link or is not a
   directory. Do this before creating any later component.
4. Create each missing directory component individually beneath the already
   proven directory, checking after each creation that it is a real directory
   and remains beneath the canonical root. Never resume traversal through the
   original unresolved joined path.
5. Resolve and validate the final target with the same regular-file and
   no-symlink rules already used by reads, updates, deletes, and digests.
6. If a later validation or creation step fails, remove any still-empty parent
   directories created by this operation in reverse order. Never remove a
   pre-existing path or a non-empty directory.

Keep the filesystem race boundary explicit. A component-by-component solution
using `std` closes the reproduced pre-existing-symlink defect. If the catalog
root must also be secure against a concurrently hostile process replacing
directories between checks and use, path-based `std` operations are
insufficient; use directory-handle-relative, no-follow/beneath operations on
platforms that provide them and define the strongest equivalent elsewhere.
Do not claim protection against concurrent directory replacement without such
an implementation and test.

### Required regression test

Add a Unix test beside the existing containment tests that:

- creates a catalog root and a separate outside directory;
- places `linked` in the catalog root as a symlink to the outside directory;
- calls the public `create_entry` API for
  `linked/created-outside/planets.yaml`;
- expects `WriteError::NotContained`;
- proves that neither `created-outside` nor `planets.yaml` was created outside;
  and
- proves that the catalog root, target, revision, events, and accepted-command
  history are unchanged when the case is also exercised through
  `CatalogAuthority::submit`.

Also cover at least two missing directory levels and retain the existing direct
symlink-parent, symlink-target, exclusive-temporary-file, and intentionally
symlinked-root cases.

Delivered as
`write::tests::containment::a_missing_descendant_below_a_symlinked_parent_creates_nothing_outside`,
`::several_missing_levels_below_a_symlinked_parent_create_nothing_outside`,
`authority::tests::a_create_below_a_symlinked_parent_is_refused_and_touches_nothing_outside`,
and `write::tests::undoing_a_resolution_removes_only_the_empty_directories_it_created`.
Every earlier containment case is retained. The two new writer tests were
confirmed to fail against a deliberately reintroduced `create_dir_all` on the
assembled path and to pass against the correction.

Rollback after a *successful* directory creation is reachable only through a
concurrent race, so it is covered by the direct `undo_created_directories` test
— which proves both that an empty directory it created is removed and that a
non-empty one never is — rather than by an end-to-end scenario.

## Defects

The two defects below describe the behaviour before this correction.

### An update can rename onto an existing identity

`CatalogAuthority::Create` checks the proposed identity but exempts an existing
`Invalid` claimant, while `Update` writes a replacement before checking whether
another source already owns the new identity. Reload then classifies one
claimant as a duplicate after the user's file has already changed, while the
command reports success. The emitted `CatalogChange::Updated` also carries only
the new identity, so a consumer applying change events cannot know that the old
identity disappeared.

Identity ownership must be checked against the current catalog snapshot before
any file effect. An invalid or unavailable entry whose metadata supplies a
valid identity still reserves that identity: preserving broken user data must
not make name resolution depend on which claimant happened to validate.

### Lexically safe paths may still escape through symlinks

The writer rejects absolute paths and `..`, but currently joins otherwise
normal components directly beneath the root. A symlinked parent directory can
therefore redirect create/update/delete or temporary-file creation outside the
catalog root. A predictable temporary name opened with ordinary create
semantics can also follow a pre-existing symlink.

The public claim that adapter-controlled paths stay inside the catalog root
requires filesystem-aware containment, not only lexical normalization.

## Required changes

### 1. Make identity changes collision-safe

- Before create or update writes anything, reject a proposed identity already
  claimed by a different source/document. Updating a document without changing
  its identity remains valid.
- Include structurally recoverable invalid and unavailable entries in collision
  ownership when they expose a valid catalog/template identity.
- Represent a successful rename explicitly, either with a dedicated change or
  an update carrying both previous and current identities. Event consumers must
  be able to remove the old row and insert the new one without guessing or
  performing an undocumented full refresh.
- Ensure a rejected collision records no accepted command outcome and advances
  neither catalog revision nor event history.

### 2. Enforce physical filesystem containment

- Establish the configured catalog root explicitly and reject existing path
  components or target files that traverse symbolic links outside it. Handle a
  missing leaf without weakening checks on its existing ancestors.
- Apply the same rule to reads, creates, updates, deletes, file-digest checks,
  and sibling temporary files. Loading may diagnose a symlinked catalog entry,
  but must not silently treat bytes outside the root as owned catalog data.
- Create temporary files without following an existing path and without
  clobbering another process's temporary file. Preserve same-directory atomic
  replacement and cleanup on failure.
- Keep platform behavior explicit. Unix symlink tests may be conditionally
  compiled, while other supported platforms need the strongest equivalent
  containment available from their filesystem APIs.

## Acceptance criteria

- Renaming template A to template B's identity is rejected before disk changes;
  both entries, the projection, revision, events, and accepted-command history
  remain byte-for-byte/semantically unchanged.
- The same collision is rejected when B is unavailable or is invalid but has a
  recoverable valid identity.
- A successful non-colliding rename reports both its old and new identities,
  and a consumer applying the event reaches the authority's current listing.
- Create, update, delete, digest, and reload cannot follow a symlinked parent or
  target outside the catalog root. Tests prove that an external sentinel file
  is unchanged and that no external directory or file is created, including
  when one or more requested parent components are missing below the symlink.
- A pre-existing temporary-path symlink/file cannot be followed or overwritten;
  the operation either chooses a safely exclusive sibling or fails without
  changing the target.
- Existing sibling preservation, stale-digest rejection, idempotent command
  replay, malformed-entry isolation, and materialization tests remain green.
- `cargo fmt --all -- --check`,
  `cargo clippy --locked -p kagami-catalog --all-targets -- -D warnings`,
  `cargo test --locked -p kagami-catalog --all-targets`, and
  `make docs-check` pass.

## Non-goals

- Implementing the experiment document authority or committing an
  `ObjectCandidate` into a document.
- UI or MCP transport adapters.
- Catalog-variable capture into workload manifests.
- Adding dimension inference to the shared variables subsystem.
- Supporting writes outside the configured catalog root.
