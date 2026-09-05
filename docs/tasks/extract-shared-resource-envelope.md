# Extract the shared Kubernetes-style resource envelope

Status: **ready; the catalog prerequisite for the Kagami migration is accepted**

Related decisions:
[ADR 0013](../adr/0013-cluster-formation-and-node-identity.md),
[ADR 0008](../adr/0008-catalog-templates-instantiate-self-contained-objects.md),
[ADR 0010](../adr/0010-content-addressed-workload-closure-and-portable-bundles.md)

Related tasks:
[shared workload format](define-and-adopt-shared-workload-format.md) and
[Kagami object catalog](implement-kagami-object-catalog.md)

## Outcome

Add a dependency-light shared crate, provisionally named `orishu-resource`, for
the common human-readable resource shape used by Orishu and Kagami:

```yaml
apiVersion: <domain/version>
kind: <resource kind>
metadata: <resource-specific metadata>
spec: <resource-specific desired or descriptive data>
status: <optional resource-specific observed state>
```

Orishu workload, cluster, and node resources and Kagami object-template
resources use the same structural envelope and discriminator parsing rather
than maintaining parallel definitions. Each resource domain still owns its
identity, metadata schema, validation, authority, mutability, persistence,
status, and canonicalisation rules.

This is a library-boundary refactor, not a decision to make the system behave
like Kubernetes. The shared shape makes resources familiar to read, print,
save, and inspect; it does not create a Kubernetes API server, CRD mechanism,
generic CRUD semantics, or a universal resource authority.

## Current state

`crates/orishu/src/model/manifest.rs` currently defines:

- unvalidated string `Name` and `ID` wrappers;
- an Orishu-oriented `ObjectMeta`;
- `Manifest<T, S>` with `apiVersion`, `kind`, metadata, spec, and optional
  status; and
- `ParseError`, whose nominally generic `InvalidResource` display text is
  hard-coded to `orishu.dev/v1` and `Workload`, while `InvalidSpec` belongs to
  workload-domain validation.

The generic envelope is used by Orishu workload, cluster, and node type aliases.
Checkpoint and result models also reuse the current manifest `Name` and `ID`
types. Workload parsing lives on the workload alias and performs
resource-specific discriminator and spec validation.

`crates/kagami-catalog/src/document.rs` separately defines an `Envelope` and
`TemplateDocument` with the same leading resource shape but intentionally
different metadata. Its loader supports bounded multi-document YAML,
per-document error isolation, early discriminator checks, and canonical
template encoding. Those catalog behaviors must remain in `kagami-catalog`.

The second independent consumer now exists, so the extraction gate described
by the shared-workload task has been met.

## Semantic boundary

Sharing an envelope does not mean sharing resource lifecycle:

| Resource | Authority and lifecycle |
| --- | --- |
| Workload | Immutable submitted definition plus a separately governed runtime projection; workload identity must follow the shared workload decision, not generic metadata alone. |
| Cluster | Synthetic runtime projection of current formation state. It is not a durable operator-authored `cluster.yaml` and has no generic create/update/delete lifecycle. |
| Node | Cluster-owned membership projection; labels are not membership identity. |
| Kagami object template | Client-owned editable catalog data. Its metadata, bounded YAML-stream loading, validation, canonical fingerprint, and write authority stay in `kagami-catalog`. |
| Future experiment/plugin resources | May reuse the envelope only after defining their own authority, identity, versioning, and persistence contracts. |

ADR 0013 is the authoritative current decision for the cluster row. Historical
Orishu decision 005, `Cluster manifest is a synthetic resource rather than a
user-authored durable object`, is useful migration evidence and reaches the
same conclusion: `metadata.id` represented formation identity,
`metadata.name` was a reusable human label, and the full cluster resource was
primarily a client/API projection rather than a peer-protocol source of truth.
Do not revive the historical field spellings where current ADR 0013 and
`orishu-identity` have since introduced stronger domain types.

## Required design

### 1. Keep the envelope structural and generic

- Define a serde-compatible generic resource type whose metadata, spec, and
  optional status are type parameters. A shape such as
  `Resource<M, S, T = NoStatus>` is preferable to fixing all consumers to the
  current Orishu `ObjectMeta`.
- Define a small discriminator/header type that can inspect `apiVersion` and
  `kind` before decoding a resource-specific body. The generic crate must not
  hard-code `orishu.dev/v1`, `kagami.catalog/v1`, `Workload`, or
  `ObjectTemplate`.
- Preserve the external spelling `apiVersion` and the established field order
  where canonical encoders or fixtures depend on it.
- If `ApiVersion` and `Kind` become newtypes, bound and validate their syntax at
  construction. The owning domain decides which values it supports.
- Do not add dynamic `serde_json::Value` or `serde_yaml::Value` as the normal
  domain representation. Unknown resource dispatch may inspect a bounded
  header, then decode into a typed resource.

### 2. Do not universalise metadata or identity

- Keep Kagami's catalog/name/description/annotations metadata type in
  `kagami-catalog`; do not disguise `catalog` as a Kubernetes namespace.
- Either keep Orishu `ObjectMeta` in `orishu` or make a generic shared metadata
  helper only if its type parameters preserve the resource-specific name and ID
  newtypes. A generic `String` ID must not replace `FormationId`, `NodeId`,
  workload digest, or other identity-bearing domain types.
- Audit current uses of manifest `Name` and `ID` in workload, checkpoint, and
  result models. Migrate them deliberately to the correct domain identity type
  or preserve them behind a compatibility alias; do not silently reinterpret
  persisted or protocol fields.
- Do not resolve the current `metadata.id` versus protocol `metadata.uid`
  vocabulary incidentally. Record it as coordination with S-IDENTITY and
  O-API-SHAPE, preserving current wire behavior until that contract is decided.
- Labels and annotations are descriptive unless a resource-specific decision
  explicitly makes them identity-bearing. Generic serialization must not imply
  that they participate in workload digests or formation identity.

### 3. Separate syntax, discriminator, and domain validation

- The shared error model may report invalid UTF-8, size/shape limits, syntax,
  missing envelope fields, and unsupported/mismatched discriminator values.
  Error text must report the caller-supplied expected and actual resource type,
  not mention Workload unconditionally.
- Workload spec validation, catalog component/expression validation, cluster
  projection invariants, and node membership validation remain structured
  errors in their owning crates.
- Parsing entry points must accept explicit byte, nesting, string, and
  collection limits or operate only after an owning bounded input layer. Do not
  introduce a convenient unbounded network-facing parser.
- JSON and YAML are human-facing codecs, not automatically canonical identity
  encodings. Workload hashing continues to use the canonical codec selected by
  S-WORKLOAD; catalog fingerprints continue to use catalog canonicalisation.
- Multi-document streams, filesystem discovery, atomic writes, symlink
  containment, and per-document catalog recovery remain outside this crate.

### 4. Keep dependencies and layering small

- `orishu-resource` must not depend on `orishu`, `kagami-catalog`, application
  crates, async runtimes, networking, filesystems, UI, or workload execution.
- Prefer only `serde` in the core. Put JSON/YAML helpers behind justified
  features or leave concrete codec selection with consumers if that keeps the
  dependency graph and error ownership clearer.
- Add a dependency-boundary test using workspace metadata so later changes
  cannot pull runtime or application dependencies into the shared crate.
- The crate exposes values and pure parsing/validation helpers; authorities and
  IO adapters consume it.

## Implementation slices

### 1. Freeze compatibility fixtures and the type map

- Inventory every current `manifest::{Manifest, ObjectMeta, Name, ID,
  ParseError}` use and classify it as structural, Orishu metadata, domain
  identity, codec, or domain validation.
- Add or identify golden JSON/YAML fixtures for workload, synthetic cluster,
  node, and object-template resources before moving types.
- Record which current wire spellings must remain byte/semantically compatible
  and which prototype-only constructors or error messages may change.
- Coordinate public Orishu model edits with S-WORKLOAD, S-IDENTITY, and
  O-API-SHAPE. Do not combine this refactor with the new workload schema.

### 2. Introduce `crates/orishu-resource`

- Add the generic envelope, header/discriminator, constructors/accessors, and
  generic syntax/discriminator errors agreed above.
- Derive or implement serde without imposing `Default`, `Clone`, or other
  unnecessary bounds on consumer types.
- Test exact field spellings, absent versus present status, typed round trips,
  unknown future versions/kinds, unknown fields according to the selected
  policy, malformed input, and declared bounds.
- Add rustdoc examples using both an Orishu resource and a Kagami-style
  resource-specific metadata type.

### 3. Migrate Orishu resources without changing semantics

- Replace the structural definition in `crates/orishu/src/model/manifest.rs`
  with imports or compatibility re-exports from `orishu-resource`.
- Keep resource-specific constants and validation near workload, cluster, and
  node models. Constructors must require or derive the correct kind instead of
  retaining the current placeholder `kind: "()"` behavior.
- Preserve workload, cluster, and node JSON/YAML behavior and client API tests.
- Preserve a temporary source-compatible re-export where useful, mark the
  migration path, and remove it only after all workspace consumers migrate.
- Verify checkpoint/result provenance still uses the correct workload identity
  and human name rather than inheriting a generic resource ID assumption.

### 4. Adopt the envelope in `kagami-catalog`

- Reuse the shared resource header and structural envelope in
  `TemplateDocument` without moving `MetadataDocument`, `SpecDocument`, or
  catalog validation into the shared crate.
- Preserve `apiVersion: kagami.catalog/v1`, `kind: ObjectTemplate`,
  `deny_unknown_fields`, field-located diagnostics, deterministic canonical
  bytes, YAML-stream behavior, malformed-entry isolation, and all file-authority
  protections.
- The catalog containment correction this slice had to wait behind is
  accepted, so catalog load/document/write code is free to edit. Preserve its
  component-by-component path containment and exclusive temporary-file
  creation rather than reintroducing a path assembled from the root and
  untrusted components.
- Prove all shipped files under `etc/catalogs` load to the same templates and
  fingerprints before and after the refactor.

### 5. Reconcile CLI, protocols, and documentation

- Make `orishuctl` human-readable output and saved resource files use the
  shared typed resource representation where those commands expose a resource.
  Do not fabricate editable cluster configuration from the synthetic cluster
  projection.
- Update protocol examples only where implementation behavior actually changes;
  otherwise use fixtures to prove this extraction is wire-neutral.
- Document the structural convention once and link the workload, cluster,
  node, catalog, experiment, and plugin formats to it as they adopt it.
- Update `AGENTS.md` repository mapping after the crate exists, not as part of
  this planning-only task.

## Acceptance criteria

- One dependency-light crate owns the generic resource envelope and header;
  neither `orishu` nor `kagami-catalog` carries a parallel structural copy.
- Orishu workload, cluster, and node resources and Kagami object templates
  round-trip through the shared envelope with their existing external
  `apiVersion`, `kind`, `metadata`, `spec`, and optional `status` spellings.
- Existing supported resource fixtures retain their semantics. Any intentional
  wire change is separately versioned and documented rather than hidden in the
  extraction.
- A synthetic cluster resource cannot be parsed or manipulated as durable
  operator-authored cluster configuration merely because it shares the
  envelope.
- Kagami catalog metadata remains catalog-specific, and catalog YAML streams,
  diagnostics, canonical fingerprints, authority behavior, and shipped
  examples remain unchanged.
- Unknown versions and kinds are rejected with bounded structured errors that
  report expected and actual discriminators without hard-coded Workload text.
- Resource-specific validation errors remain owned by the resource domain and
  are not flattened into generic strings by `orishu-resource`.
- Formation, node, workload, template, and future plugin identities remain
  distinct types with their documented authority and scope.
- The shared crate's dependency test excludes applications, UI, async runtime,
  transport, filesystem, catalog, and Orishu domain dependencies.
- Focused crate tests, Orishu model/client tests, catalog tests, protocol fixture
  tests, `cargo fmt --all -- --check`, workspace clippy, workspace tests, and
  `make docs-check` pass.

## Parallel work and dependencies

- Slices 1–3 remain independently implementable and need not touch
  `crates/kagami-catalog`.
- Slice 4 is unblocked because the catalog containment correction is accepted.
- S-WORKLOAD may specify domain identities and canonical encoding in parallel,
  but it should consume this envelope instead of introducing another generic
  manifest type. Avoid landing simultaneous incompatible public-model edits.
- O-API-SHAPE owns any change from `metadata.id` to `metadata.uid` and the
  generic client resource surface; this task preserves current wire behavior.

## Non-goals

- Implementing Kubernetes, CRDs, controllers, watches, generic CRUD, or a
  Kubernetes-compatible API.
- Giving every resource a `status`, namespace, labels, annotations, or
  system-generated ID.
- Making the cluster resource durable or operator-authored.
- Choosing workload canonical identity, artifact closure, or distribution
  format; S-WORKLOAD owns those decisions.
- Moving catalog loading, writing, authority, materialization, expressions, or
  filesystem behavior into the shared crate.
- Defining experiment or plugin resources before their authority and schema
  tasks are accepted.
- Changing current API/wire formats merely to resemble Kubernetes more closely.
