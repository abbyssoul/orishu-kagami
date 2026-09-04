# Define and adopt the shared workload format

Status: **ready; foundational priority**
Decisions: [ADR 0005](../adr/0005-author-numeric-values-as-unit-aware-expressions.md),
[ADR 0007](../adr/0007-share-expression-semantics-with-workload-resources.md),
[ADR 0009](../adr/0009-execute-workloads-as-sandboxed-portable-programs.md),
[ADR 0010](../adr/0010-content-addressed-workload-closure-and-portable-bundles.md)

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
- the **workload component**, which is the sandboxed executable physics;
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
  expression language version, runtime requirements, root workload component,
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

- Keep the initial implementation in `crates/orishu`, which is already the
  shared model and client seam used by Kagami and Orishu applications. Extract
  a narrower crate only if a second independent dependency boundary makes that
  complexity real.
- Introduce domain types such as `WorkloadManifest`, `WorkloadDigest`,
  `ArtifactDigest`, `ArtifactDescriptor`, `ArtifactRole`, and
  `WorkloadClosure`; do not expose transport URLs through them.
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
- Define one root lifecycle component for the first profile. It may encapsulate
  reusable kernels, but every code dependency required at execution is pinned
  in the closure and no domain/template name selects executable code implicitly.

### 4. Make Kagami produce the shared format

- Define a pure experiment-to-workload compilation boundary whose output is
  the shared `WorkloadManifest` plus artifact candidates, not Kagami UI or
  document types leaking into Orishu.
- Resolve selected simulation plugins through Kagami's validated inventory and
  compile exact schema/model identities and component descriptors. Never emit
  a plugin installation path, source URL, mutable tag, or “use installed
  version” instruction into the workload.
- Before the full experiment authority exists, add a minimal fixture/builder
  path proving Kagami can construct and serialize a valid workload with a
  pinned component and initial conditions through the shared API.
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
  digest, resolved-parameter fingerprint, and required artifact set before
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
- A golden workload containing a pinned component and initial conditions has
  identical canonical bytes and root digest when produced by the Kagami-side
  fixture and read by Orishu.
- Changing initial conditions changes their descriptor and root workload digest
  but not the component digest. A worker that has the component requests only
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
  validates the component artifact consumed by that engine.
- Completing Kagami's experiment editor, catalog UI, or MCP surface.
- Treating a distribution archive, location, mutable tag, runtime status, or
  cluster epoch as workload identity.
