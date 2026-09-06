# ADR 0010: Use content-addressed workload closures and portable bundles

Status: **accepted**
Date: **2026-09-04**

Refined by: [ADR 0024](0024-orishu-orchestrates-a-workload-component-graph.md)

## Context

A workload needs a compute definition, one or more executable WebAssembly Components,
initial conditions, and potentially large geometry or supporting artifacts.
Users should be able to share a workload without operating an artifact
repository, while workers should not retransmit a rarely changing kernel every
time initial conditions change.

The current `ExternalResource` model offers URI or inline data but does not
separate immutable artifact identity from location. It cannot fully express a
portable workload whose component and inputs may arrive from different sources
or already exist in a worker cache.

Three packaging strategies are plausible.

### One monolithic file

A manifest, component, and all inputs could be concatenated or archived as one
canonical file.

- It is easy to copy, archive, and submit offline.
- Any changed input creates and retransmits a new whole package.
- Kernels and common inputs cannot be independently cached or deduplicated.
- Whole-stream compression impedes range access, resumable transfer, and
  partition-aligned distribution of large scientific inputs.
- Archive ordering, timestamps, permissions, and compression settings require
  canonicalization to avoid accidental identity changes.

### A manifest with location references

Following the surface shape of Kubernetes, the manifest could name an image or
input by registry tag, URL, or local path and let each worker provision it.

- Small manifests and external stores naturally cache shared kernels.
- Execution depends on external availability, credentials, and mutable naming.
- Different workers can receive different bytes unless every reference is
  digest-pinned and verified.
- Offline sharing and long-term archival require recreating the external
  dependency set.

### A content-addressed closure with optional bundle transport

The root manifest can reference typed blobs by digest and size. Distribution
metadata outside the manifest says where matching bytes can be obtained.
Submission transfers only absent blobs, while an offline bundle can carry the
complete reachable closure.

- Logical identity is independent of packaging and location.
- Kernels and common inputs are transferred once and reused by digest.
- A full bundle remains self-contained and shareable without a registry.
- Large inputs can be chunked, ranged, verified, and distributed independently.
- The implementation must manage a content-addressed cache, closure validation,
  garbage collection/pinning, and partial-transfer recovery.

This resembles the descriptor/blob separation in the OCI specifications: an
OCI manifest references digest-addressed blobs, and an OCI Image Layout can be
transported as a directory or archive and may be complete or obtain missing
blobs externally. Orishu should borrow that shape without inheriting container
filesystem layers or requiring an OCI registry. See the
[OCI Image Layout specification](https://github.com/opencontainers/image-spec/blob/main/image-layout.md)
and
[OCI Distribution specification](https://github.com/opencontainers/distribution-spec/blob/main/spec.md).

## Decision

Represent a workload as an immutable root manifest and the transitive closure
of content-addressed artifacts it references.

Each artifact descriptor contains at least a role, digest, byte size, media
type, and relevant format/schema compatibility. It contains no URL, filesystem
path, peer identity, registry tag, or other retrieval location. Every fetched
blob is bounded and digest-verified before admission to the local store or
package instantiation.

Support both:

- thin submission, where the client and cluster negotiate missing digests and
  transfer only absent blobs; and
- a deterministic portable bundle containing the root manifest and complete
  closure for offline transfer and archival.

Importing a bundle populates the same content-addressed store used by thin and
peer transfer. The bundle byte stream is not workload identity. Exporting and
re-importing through another supported bundle encoding preserves the root and
artifact digests.

Do not require a central artifact repository. The submitting client,
authenticated workers, a local cache, and optional HTTP/OCI-style repositories
are alternative sources for identical digest-pinned bytes. Peer location is
runtime availability state and is never persisted in the workload manifest.
Source hints belong to a submission request, distribution envelope, or live
availability index and may change without creating a new workload identity.

Keep workload components, initial conditions, geometry, and other large
assets as independently addressable artifacts. Inline data is reserved for
small, explicitly bounded values where deduplication and streaming are not
useful. Large inputs may use a descriptor over partition-aligned chunks rather
than one indivisible blob.

A Kagami simulation plugin is an authoring-time distribution of schemas and
executable capability, not workload identity. Selecting an installed plugin
causes workload compilation to include its exact schema/model identities and
digest-pinned component artifacts. Plugin name, installation directory,
registry, update channel, or archive encoding never replaces those descriptors
and never selects executable physics on a worker. See
[Simulation plugins](../simulation-plugins.md).

Support multiple versioned distribution formats over the same manifest and
closure. The first portable bundle encoding is deferred until prototype
measurements compare a minimal Orishu layout with reuse of OCI Image Layout.
Any selected encoding must be deterministic, streamable with declared bounds,
safe from path traversal, and able to verify content before installation.
Whole-stream gzip is not the canonical identity or storage unit.

## Consequences

- A kernel used by many workloads is fetched and cached once, while each
  workload can pin different parameters and initial-condition artifacts.
- Built-in and third-party simulation plugins converge on the same
  content-addressed workload representation at submission.
- Users retain a one-file offline export option without making one archive the
  runtime storage model.
- Workload submission needs a missing-blob negotiation and upload path in
  addition to accepting the root manifest.
- Worker artifact storage must include immutable workload inputs and packages,
  not only checkpoint and result outputs, and must define pinning and garbage
  collection for accepted workloads.
- Signatures can bind the root manifest and its digest-referenced closure;
  workers still validate every descriptor and blob independently.
- `ExternalResource::{Image, Inline}` must evolve into typed,
  content-addressed descriptors, with source hints represented separately as
  distribution metadata and with schema migration and compatibility handling.
- Bundle encoding remains reversible while the logical manifest/artifact model
  is stable.
