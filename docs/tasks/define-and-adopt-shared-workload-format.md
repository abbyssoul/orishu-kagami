# Define and adopt the shared workload format

Status: **in progress** — slice 1 and the structural half of slice 3 are
implemented and accepted. Slice 2 is implemented **except** for its
before-allocation bound on collections, which is
[carried forward](#carried-forward-from-slice-2) to the admission path. Slices
4–7 remain. See [Increment record](#increment-record--2026-09-07).
Decisions: [ADR 0005](../adr/0005-author-numeric-values-as-unit-aware-expressions.md),
[ADR 0007](../adr/0007-share-expression-semantics-with-workload-resources.md),
[ADR 0009](../adr/0009-execute-workloads-as-sandboxed-portable-programs.md),
[ADR 0010](../adr/0010-content-addressed-workload-closure-and-portable-bundles.md),
[ADR 0024](../adr/0024-orishu-orchestrates-a-workload-component-graph.md)

## Outcome

Kagami and Orishu use one versioned workload definition and canonical codec.
Kagami produces an immutable workload manifest plus digest-addressed artifact
closure; Orishu parses, validates, identifies, and admits those exact bytes
without translating through a second application-specific schema.

Whether the selected model came from Kagami's bundled gravity/electrodynamics
plugins or a user-installed simulation plugin does not change this boundary.
The plugin is authoring input; its exact model/schema identities and workload
component artifacts become ordinary pinned workload content.

The shared model distinguishes:

- the **workload**, which is the logical manifest and complete required
  closure;
- the **component graph**, whose sandboxed executable instances, typed channels
  and deterministic step plan compose the physics;
- runtime **workload status/epoch**, which is cluster state and not part of the
  immutable workload;
- a **distribution envelope/provider**, which supplies locations or bytes but
  is not workload identity; and
- a **portable workload bundle**, which is one distribution format containing
  the complete closure.

Land the shared schema, identity, validation, and an in-memory/file round trip
first. Full peer distribution, cache eviction, and polished Kagami authoring can
build on that seam without delaying it.

## Current gap

`crates/orishu/src/model/workload.rs` is already the model imported by Kagami,
but its `ExternalResource` is only an unpinned URI or inline string. Examples
use mutable OCI tags and HTTP locations as if they were resource identity. The
generic manifest also mixes submitted desired state with optional runtime
status and system-generated metadata.

Consequently the current type cannot represent ADR 0010's closed workload,
cannot negotiate or verify missing artifacts independently, and makes it too
easy for Kagami and a worker to assign different meaning to the same apparent
manifest.

## Implementation slices

### 1. Specify the identity-bearing model

- Define the next version of the workload schema in
  [`docs/workloads.md`](../workloads.md) and the protocol documents before
  changing wire behavior. Include the compute definition, variables and
  expression language version, runtime requirements, component instances,
  typed channels, deterministic step plan, placement constraints,
  initial conditions, geometry, and other profile-declared inputs.
- Replace image-oriented terminology with a typed `ArtifactDescriptor`
  containing stable role, digest, byte size, media type, and format/schema
  compatibility. Use SHA-256 for the first version while keeping the digest
  value explicitly algorithm-tagged.
- Do not put URI, filesystem path, peer identity, registry tag, bearer
  credential, or other location in the workload manifest. Define separate
  transport/distribution types for source hints.
- Separate immutable submitted metadata/specification from the worker-assigned
  resource ID, workload epoch, phase, simulation time, partition map, and other
  runtime projection fields.
- Define which metadata is identity-bearing. Human names and labels may be
  retained in the immutable manifest, but server annotations and status must
  never affect the workload digest.

### 2. Implement one shared domain API and canonical codec

- Reuse the generic envelope from the
  [shared resource task](extract-shared-resource-envelope.md); Kagami's catalog
  is now the second independent consumer that justifies that extraction. Keep
  workload identity, artifact closure, domain validation, and canonical codec
  in the shared workload domain rather than moving them into the structural
  resource crate.
- Introduce domain types such as `WorkloadManifest`, `WorkloadDigest`,
  `ArtifactDigest`, `ArtifactDescriptor`, `ArtifactRole`, and
  `WorkloadClosure`, plus `ComponentInstanceId`, `ComponentInstance`,
  `StateChannelId`, `StepInvocationId`, and `StepPlan`; do not expose transport
  URLs through them.
- Select and document one deterministic canonical encoding for workload
  identity and exchange. JSON/YAML may remain human authoring inputs, but they
  parse into the typed model and cannot define identity through insignificant
  whitespace, key order, aliases, comments, timestamps, or platform paths.
- Hash the canonical root manifest. Because every transitive dependency is
  digest-addressed, that root digest identifies the logical workload. Add
  golden canonical-byte and digest fixtures so changes cannot occur silently.
- Bound manifest bytes, nesting, collections, strings, descriptor counts,
  aggregate declared size, and expression graphs before allocation or artifact
  retrieval.

### 3. Validate a complete closure

- Add a transport-neutral closure validator that accepts a manifest and a
  provider of candidate blobs, returns verified artifacts keyed by digest, and
  never trusts provider location or metadata.
- Check role uniqueness/cardinality, digest and exact byte size, media type,
  schema/state compatibility, workload lifecycle/component imports, expression
  resolution, dimensions, finite values, and declared resource requirements.
- Report missing, corrupt, oversized, duplicate-role, incompatible, and
  unsupported artifacts as structured errors without partially accepting the
  workload or populating a trusted cache with unverified bytes.
- Validate bounded instance/node/edge/channel counts; stable component, model,
  schema and phase identities; acyclic or explicitly versioned bounded
  schedules; channel type/dimension compatibility; complete inputs; allowed
  writers; deterministic reductions; model-family exclusivity; placement
  constraints; and per-instance/aggregate limits.
- Every code dependency required at execution is pinned in the closure and no
  domain/template name selects executable code implicitly.

### 4. Make Kagami produce the shared format

- Define a pure experiment-to-workload compilation boundary whose output is
  the shared `WorkloadManifest` plus artifact candidates, not Kagami UI or
  document types leaking into Orishu.
- Resolve selected simulation plugins through Kagami's validated inventory and
  compile exact schema/model identities, component instances, typed channels,
  phase dependencies and component descriptors. Never emit
  a plugin installation path, source URL, mutable tag, or “use installed
  version” instruction into the workload.
- Before the full experiment authority exists, add a minimal fixture/builder
  path proving Kagami can construct and serialize a valid workload with a
  minimal multi-component graph and initial conditions through the shared API.
- Integrate the variables/expression graph when its shared task lands. Kagami
  retains authored source in the manifest while materializing initial-condition
  and geometry blobs deterministically for the selected experiment revision.
- Ensure repeat compilation of identical experiment intent and artifacts
  produces identical canonical manifest bytes, artifact digests, and workload
  digest.

### 5. Make Orishu admit the exact shared format

- Replace `ExternalResource::{Image, Inline}` loading with descriptor-based
  closure resolution and validation. Remove mutable tag/URL examples from
  production tests and protocol examples.
- Make compatibility check and load call the same parser, canonicalizer,
  closure validator, expression validator, and component-policy validator.
  Checking remains read-only; loading freezes the already validated manifest
  and verified artifacts for a new epoch.
- Keep workload identity distinct from the cluster-assigned resource ID and
  epoch. Every participating worker must agree on the root digest, component
  graph/step-plan digest, component digests, resolved-parameter fingerprint,
  and required artifact set before
  reporting `Ready`.
- Persist and gossip identity-bearing manifest bytes/descriptors, never source
  URLs, local cache paths, credentials, or transient holder locations.

### 6. Establish distribution-format seams

- Define an `ArtifactProvider`/import boundary outside the domain model so thin
  submission, local directory import, portable archive import, authenticated
  peer fetch, cache hits, and optional repository fetch all yield candidate
  bytes to the same verifier.
- Implement the smallest end-to-end thin path: submit canonical manifest,
  report missing digests, upload only those blobs, verify them, then admit the
  workload. Retrying a verified digest is idempotent.
- Prototype a minimal Orishu layout against OCI Image Layout and record the
  first portable bundle encoding. A full bundle must contain the root and
  complete closure, reject path traversal and duplicate entries, and import to
  exactly the same workload digest as thin submission.
- Version distribution codecs independently of the workload schema. Multiple
  codecs may coexist and round-trip the same logical workload.
- Reuse the cluster's content-addressed chunk and availability concepts where
  they fit, but keep immutable descriptor identity separate from transient
  holders and retrieval routes.

### 7. Migrate protocol, examples, and documentation

- Update the client and peer protocols so their schemas use artifact
  descriptors and root workload digests consistently. Specify missing-blob
  negotiation, bounded upload, error taxonomy, idempotency, and authentication.
- Update CLI loading so a user can submit a human-readable manifest plus local
  artifacts, a thin canonical workload, or the first full portable bundle
  without these becoming different workload semantics.
- Define migration for the current `orishu.dev/v1` URI/inline prototype. If no
  durable compatibility obligation exists, fail it explicitly rather than
  carrying ambiguous mutable references into the new format.
- Update API examples, user stories, fixture manifests, and rustdoc together;
  remove the obsolete `image` vocabulary for non-container artifacts.

## Acceptance criteria

- `docs/workloads.md`, `CONTEXT.md`, protocol documents, public Rust types, and
  examples use the same definitions of workload, manifest, component, closure,
  bundle, distribution format, runtime status, resource ID, and epoch.
- Kagami and every Orishu application consume the same public manifest and
  artifact descriptor types from `crates/orishu`; no parallel Kagami wire model
  or conversion schema exists.
- An equivalent built-in or third-party simulation plugin compiles through the
  same workload model and admission path; bundled plugins have no hidden
  manifest or execution privilege.
- A golden workload containing at least a field-model and Dynamics component
  connected by typed channels and a deterministic step plan, plus initial conditions, has
  identical canonical bytes and root digest when produced by the Kagami-side
  fixture and read by Orishu.
- Changing initial conditions changes their descriptor and root workload digest
  but not component digests. A worker that has the components requests only
  the new/missing blobs.
- Changing only a source URL, peer, cache path, archive encoding, or blob order
  does not change workload identity; none of those locations occur in the
  canonical manifest.
- Runtime status, cluster resource ID, epoch, simulation time, and partition
  ownership cannot be serialized into or affect the immutable workload digest.
- Thin submission and portable-bundle import resolve to the same manifest,
  closure, and workload digest and produce the same compatibility/admission
  decision.
- Missing, corrupt, oversized, duplicate, incompatible, and forbidden-component
  artifacts produce bounded structured failures before workload replacement or
  guest initialization.
- Missing producers, multiple unauthorized writers, incompatible channel
  dimensions, cycles, unbounded schedules, ambiguous reductions and illegal
  placement constraints are rejected before any component initialization.
- Co-located and distributed placements of the same accepted graph retain the
  same workload identity and declared numerical semantics.
- Identical content is stored once by digest and retries are idempotent. An
  accepted workload pins its required blobs against garbage collection until
  unload and retention policy permit release.
- The old unpinned URI/inline schema is either explicitly migrated with pinned
  bytes or rejected; it is never silently accepted as the new workload format.
- `make fmt-check`, `make lint`, `make test`, `make docs`, and
  `make docs-check` pass.

## Non-goals

- Implementing every future distribution codec or requiring an external
  registry.
- Building the complete peer scheduling, repair, or cache-eviction policy
  before the shared manifest and verifier land.
- Implementing the WebAssembly execution engine itself; this task defines and
  validates the graph, plan and component artifacts consumed by that engine.
- Completing Kagami's experiment editor, catalog UI, or MCP surface.
- Treating a distribution archive, location, mutable tag, runtime status, or
  cluster epoch as workload identity.

## Increment record — 2026-09-07

The first independently reviewable domain increment landed: slices 1 and 2, plus
the structural half of slice 3.

### What exists

`crates/orishu-workload`, a dependency-light crate depending only on
`orishu-resource`, `serde`, `sha2`, `thiserror`, and the two authoring codecs.
It is deliberately **not** a module in `crates/orishu`: that crate links
`reqwest`, `tokio`, `http` and `chrono`, and Kagami's library crates must be
able to compile an experiment into a workload without acquiring any of them.
`tests/dependencies.rs` enforces the budget against the resolved graph.

- `WorkloadManifest` = the shared envelope over `WorkloadMeta` and
  `WorkloadSpec`, with `NoStatus` and `DenyUnknown`. Runtime status is
  structurally unrepresentable rather than skipped, and `WorkloadMeta` has no
  `uid` or `namespace`, so the cluster-assigned identifier cannot reach the
  digest.
- `ArtifactDigest` (algorithm-tagged) and a distinct `WorkloadDigest`, with no
  conversion between them.
- `ArtifactDescriptor` with role, digest, exact size, media type and schema
  compatibility, and no location field of any kind.
- `ComponentInstance`, `StateChannel`, `StepInvocation`, `StepPlan`,
  `PlacementConstraint`, and their validated id types, following the field
  spellings `docs/protocol-client.md` had already committed to.
- A hand-written deterministic CBOR codec (RFC 8949 §4.2), **both directions**,
  with golden bytes and digests under `tests/fixtures/`, blessed with
  `BLESS_WORKLOAD_FIXTURES=1`. `manifest_from_canonical_bytes` is bounded and
  strict: every construct outside the profile is refused rather than
  normalised. Round-tripping each fixture is what shows the encoding is
  injective, which is an identity-correctness property rather than a
  convenience — an encoder that dropped a field would give two workloads one
  digest.
- A transport-neutral `validate_closure` over a `BlobSource`, in two phases: a
  pure pass over the manifest, then verification, reached only if the first
  found nothing. Verification streams through `BlobVerifier` and holds no
  bytes; a `VerifiedClosure` records what was verified, not contents.
- `Limits`, with manifest bytes checked before parsing, every collection and
  string bound applied before any blob is requested, and the diagnostic list
  itself bounded.

### Decisions taken

- **Deterministic CBOR, not JCS or a bespoke format.** CBOR is already the
  canonical client payload format, so this adds no second codec. The encoder is
  hand-written over an explicit value tree rather than serde-derived, so a
  serialization attribute cannot silently change what a workload commits to.
- **`orishu.dev/v2`, permanently.** The superseded prototype already answers to
  `orishu.dev/v1`, so this version keeps exactly one parser per discriminator.
  It is *not* renamed when the prototype is deleted: the discriminator is inside
  the canonical bytes, so renaming it would change every workload's digest and
  break every identity, signature, and provenance record at once. Deleting the
  prototype frees the string `orishu.dev/v1`; it does not make this format that
  string. (An earlier version of this record said otherwise; that would have
  been a silent identity change and was wrong.)
- **No migration.** Nothing has been submitted against the prototype, so there
  is no compatibility obligation and no legacy parser was written.
- **Non-finite floats are rejected at construction**, via `FiniteF64`, rather
  than at encoding time. Canonical encoding is therefore total over the typed
  model: a manifest that exists can always be identified.
- **Role is not part of descriptor conflict detection.** One blob may serve two
  roles and is stored once; size, media type and schema are claims about the
  bytes and may not contradict.

### Carried forward from slice 2

Slice 2 requires bounding "manifest bytes, nesting, collections, strings,
descriptor counts, aggregate declared size, and expression graphs **before
allocation or artifact retrieval**". Everything there is satisfied *except* the
before-allocation half for collections, so slice 2 is not claimed as complete.

What holds today:

- **Before allocation:** manifest bytes — checked against the input length
  before parsing, before canonical decoding, and *while encoding*, which stops
  at the point it would exceed the budget rather than completing and then being
  measured; nesting depth; the canonical decoder's value count; and every
  validated name and role, whose constructors and serde visitors check the
  borrowed `&str` before copying it.
- **Before retrieval:** every collection and string bound, the descriptor
  counts, and the aggregate declared size — all decided from the manifest alone,
  behind a gate a manifest must pass before a provider is consulted.

What does not: **collection counts during deserialization**. `serde` builds a
`Vec` or `BTreeMap` and the count bound is applied to the result. Total
allocation stays proportional to an input already capped at 4 MiB, so this is a
looseness rather than an unbounded path — but it is not what the slice asks for.

Closing it needs a counting deserializer that refuses an over-long collection as
it reads. That is invasive, and its value is realised at a network-facing
boundary that does not exist yet, so it is tracked with **slice 5 (Orishu admits
the exact shared format)** rather than done speculatively here. Slice 5 cannot
be accepted without it.

### Deferred, and by whom

- **Within slice 3:** expression resolution, physical dimension compatibility,
  finite-value numerical rules beyond `FiniteF64`, model-family exclusivity,
  per-model policy, hardware-requirement matching, and placement feasibility
  against a real cluster. A successful `validate_closure` is a structural
  statement, not a claim that the physics is admissible.
- **Domain schema:** anisotropic and segmented discretisation, and unit-typed
  rather than canonical-SI quantities. `uom` is deliberately excluded from the
  shared crate; porting the prototype's rules is its own change.
- **Expression-bearing fields:** owned by the variables integration. A manifest
  carries resolved scalars until it lands.
- **Slices 4–7** are untouched: Kagami compilation, Orishu admission, the
  distribution seams, and the protocol/CLI migration.

### Review remediation — 2026-09-07

A review of the increment raised four findings, all valid and all addressed.

1. **Bounds were declared but not fully applied.** `max_labels` and
   `max_text_bytes` had no call sites; collection bounds were checked but did
   not stop retrieval; and verified blobs were cloned into memory while the
   limits advertised a 64 GiB artifact.

   Validation is now two phases with a hard gate between them: everything a
   manifest can get wrong on its own is decided first, and a manifest that
   fails never reaches the provider — asserted by a `BlobSource` that panics if
   consulted. `max_labels` and `max_text_bytes` are enforced, the latter over
   every map that can hold a `ScalarValue::Text`. `BlobSource` became a
   streaming interface: candidates arrive in chunks through `BlobVerifier`, are
   hashed as they go, and are never materialised; a `VerifiedClosure` holds
   descriptors and roles rather than bytes, which is what makes the 64 GiB
   default honest. A source supplying more than declared is cut off at the
   declared length. Diagnostics are bounded by `max_reported_errors`, and a
   truncated report says so instead of reading as exhaustive.

   One part is deliberately *not* claimed: deserialization itself is bounded by
   the input byte cap checked before parsing, not by a counting deserializer.
   The collection bounds are structural policy applied immediately after the
   parse. Tightening allocation during deserialization is tracked with the
   network-facing admission path that would justify it.

2. **Ownership and graph-profile invariants were incomplete.** Ownership is
   spelled twice — `StateChannel.owner` and `ComponentInstance.stateOwnership`,
   both committed to by `docs/protocol-client.md` — and the two were never
   reconciled. Now they must agree exactly; an owned channel must declare
   `reduction: single`; owned state must have exactly one writer, and not zero;
   ownership implies a single writer independently of the declared reduction;
   and `stepPlan.profile` must equal `compute.workloadGraphProfile`. These are
   structural, not the scientific policy this increment defers.

3. **Descriptor conflicts were decided from the verified set**, so a
   contradiction was invisible when the blob it concerned was missing or failed
   its digest — a submitter could hide one by withholding a blob. Conflicts,
   role cardinality, and both byte budgets are now computed from the manifest
   alone, in phase one.

4. **The codec only encoded.** `manifest_from_canonical_bytes` closes it. The
   independent strict reader in `tests/support` is kept rather than replaced:
   its value is precisely that it shares no code with the encoder, so an
   encoder and decoder that were jointly wrong in a compensating way would
   still be caught.

### Review remediation — 2026-09-08

A second review raised three further findings, all valid.

1. **The decoder accepted non-canonical spellings.** The encoder omits an empty
   collection, but the decoder accepted one written out explicitly — so two byte
   strings decoded to one manifest, and re-encoding reproduced only one of them.
   That directly contradicted the injectivity the codec had just been documented
   to have: the digest over the other byte string would have named a workload
   nobody could rebuild.

   Fixed in two layers. An explicitly encoded empty list or map is now refused
   where it occurs, with an error saying how emptiness is spelled. Then
   `manifest_from_canonical_bytes` re-encodes its result and requires the bytes
   to match — a backstop that holds for any second spelling *not* on the known
   list, so a future field whose decoding loses a distinction cannot silently
   give two workloads one digest. Both layers were verified to catch the
   original bug independently. `decode` also now checks `max_manifest_bytes`
   before reading, which is what makes the collection-header bound meaningful:
   a header can claim no more than the input holds, but only if the input is
   itself bounded.

2. **Slice 2's before-allocation bound was deferred while the status claimed
   the slice was accepted.** The status was wrong, and is corrected above under
   [Carried forward from slice 2](#carried-forward-from-slice-2). The part that
   could be closed cheaply was: validated names and roles now check the borrowed
   value before copying it, in both their constructors and their serde
   visitors — the derive's `try_from = "String"` had been allocating every
   authored name, oversized ones included, before anything looked at its length.

3. **`orishu.dev/v2` was documented as becoming `v1` later.** It will not. The
   discriminator is inside the canonical bytes, so renaming it would change
   every workload's digest and break every identity, signature, and provenance
   record at once. Deleting the prototype frees the string `orishu.dev/v1`; it
   does not make this format that string. The claim is removed from the code
   comment and from the decision above.

### Review remediation — 2026-09-08 (second round)

A third review raised two findings, both valid.

1. **The encoder could produce bytes its own decoder refused.** `decode`
   enforced `max_manifest_bytes`; `encode` did not. Under a tight limit — or
   with a large programmatically built manifest — encoding and
   `workload_digest` succeeded while decoding the result failed, so a workload
   could be given an identity that could never be read back.

   Every append now goes through a guarded `Output` that stops at the point the
   budget would be exceeded, so encoding is bounded in allocation as well as in
   result, and the size it *would* have reached is honestly reported as unknown
   (`EncodedTooLarge`, distinct from `TooLarge` for exactly the reason
   `SizeOverrun` is distinct from `SizeMismatch`). A symmetry test sweeps the
   limit across the natural encoded size and asserts that whatever encodes under
   a bound decodes under it; a second test asserts a digest is never taken over
   bytes a reader would refuse.

2. **`ArtifactRole` still derived `Deserialize` through `String`**, contradicting
   the claim above that every validated name *and role* checks a borrowed value.
   The claim was wrong. The visitor is now factored into one
   `ids::deserialize_name` used by both the macro and `ArtifactRole`, so the two
   cannot drift into validating at different moments, and a test asserts the
   ordering on both the constructor and the authoring path.

### Cleanup done here

The prototype's mutable-URL examples were purged: `oci://` and `https://`
placeholders in `crates/orishu/src/model/workload.rs`, `tests/resource_wire.rs`,
the `workload-*` wire fixtures, and `docs/protocol-client.md` are now
unresolvable `urn:orishu:superseded-prototype-*` values, so nothing in the
repository advertises a mutable reference as if it were a workload dependency.
`ExternalResource` and its holders survive until slice 5, marked superseded in
rustdoc.
