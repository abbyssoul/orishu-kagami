# Extract the shared Kubernetes-style resource envelope

Status: **implemented and accepted**. `crates/orishu-resource` owns the
envelope, both consumers are migrated, and the verification findings recorded
below are resolved and covered by regression tests.

The structural convention is documented once in
[the shared resource envelope](../resource-envelope.md). What was implemented,
what changed on the wire, and what is deliberately still open are recorded in
[Implementation record](#implementation-record) below.

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

## Starting state

This section describes the code as it was before the work landed; see
[Implementation record](#implementation-record) for what it is now.

`crates/orishu/src/model/manifest.rs` defined:

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
  (The spelling was subsequently settled in favour of `uid`; see "Still open,
  deliberately".)

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

## Implementation record

### What landed

`crates/orishu-resource` owns `Resource<M, S, T = NoStatus, P = AllowUnknown>`,
`ResourceHeader`, the bounded `ApiVersion`/`Kind` new-types, and structural
errors only. It depends on `serde` and nothing else.

- `crates/orishu/src/model/manifest.rs` keeps `ObjectMeta`, `Name`, `ID` (since
  renamed to `ResourceUid`), `MANIFEST_API_VERSION`, and `ParseError`;
  `Manifest<T, S>` is now a type alias for the shared envelope over
  `ObjectMeta`.
- `crates/kagami-catalog/src/document.rs` keeps `MetadataDocument`,
  `SpecDocument`, and every value type; `TemplateDocument` is a type alias for
  the shared envelope with `NoStatus` and `DenyUnknown`. `Envelope` is replaced
  by `ResourceHeader`.

The unknown-field policy differs by consumer and is preserved exactly: Orishu
resources accept unknown top-level fields, catalog documents refuse them. That
required hand-written serde on the envelope, since `deny_unknown_fields` cannot
be conditional in a derive.

### Wire compatibility

Byte-compatible, and pinned by fixtures:

- Workload, cluster, node, checkpoint, and result encodings —
  `crates/orishu/tests/resource_wire.rs` with golden JSON and YAML.
- All 48 shipped `etc/catalogs` templates' canonical fingerprints —
  `crates/kagami-catalog/tests/fixtures/shipped_fingerprints.txt`, captured
  before the catalog migration and unchanged after it.
- The hand-written serde against the two derives it replaced —
  `crates/orishu-resource/tests/envelope.rs` keeps byte-for-byte copies of both
  previous definitions and asserts identical encoding, decoding, and
  accept/reject behaviour.

Intentional, documented changes:

- `cluster::manifest` and `node::manifest` emit `kind: "Cluster"` and
  `kind: "Node"` instead of the previous placeholder `kind: "()"`. `"Cluster"`
  is what the client protocol already documented; `NODE_MANIFEST_KIND` already
  existed but was unused. Nothing deserializes or validates these kinds, and
  no worker constructs either resource.
- A syntactically malformed discriminator (`apiVersion: "@@@"`, an oversized
  `kind`) is now a decode error rather than a discriminator mismatch. Both
  were already errors; only the variant changed.
- `ParseError::InvalidResource` carries an `UnexpectedDiscriminator` rather
  than two bare strings, so its message reports the caller's expected pair
  instead of a hard-coded `orishu.dev/v1` / `Workload`.

### Source-compatibility notes

Moving the envelope into another crate makes an inherent `impl` on the alias
illegal (E0116), so the following became free functions in their owning module:

| Before | After |
| --- | --- |
| `workload::Manifest::{parse, parse_bytes, from_reader}` | `workload::{parse, parse_bytes, from_reader}` |
| `cluster::Manifest::new(name, spec)` | `cluster::manifest(name, spec)` |
| `TemplateDocument::new(metadata, spec)` | `kagami_catalog::document::new(metadata, spec)` |

`ObjectMeta::new` is public and infallible; it previously returned
`Result<_, Infallible>`.

### Still open, deliberately

- ~~**`metadata.id` versus `metadata.uid`.**~~ **Resolved 2026-09-07**, after
  this task, by the O-API-SHAPE follow-up: `uid` is canonical, `ID` is now
  `ResourceUid`, the node resource projects its `NodeId` into `metadata.uid`,
  and the `id` spelling is neither written nor read. The divergence note is
  removed from `docs/protocol-client.md`.
- **Canonical workload identity.** `checkpoint::Record` and `result::Record`
  still carry workload provenance as `manifest::Name` / `manifest::ResourceUid`
  under the `workloadName` / `workloadId` spellings. They were audited as workload
  provenance — not formation, node, run, or artifact identity — and pinned by
  fixture. S-WORKLOAD owns replacing them with a canonical workload identity.
- **Bounded Orishu parsing.** `workload::from_reader` is still unbounded, as
  it was before. `orishu-resource` deliberately offers no parser, so a
  network-facing caller must supply its own byte and nesting limits. Tracked
  with the workload admission path rather than resolved here.

## Verification review — 2026-09-07

The extraction, consumer migrations, dependency boundary, catalog
fingerprints, and Orishu wire fixtures were reviewed and their focused tests
pass. One acceptance blocker and one documentation/test refinement remain:

1. `ApiVersion::new` and `Kind::new` currently accept `impl Into<String>` and
   call `into()` before `check`. Passing an untrusted borrowed `&str` therefore
   allocates and copies the entire value before enforcing `MAX_LEN`, contrary
   to this task's hostile-input rule and the public rustdoc claim that the
   borrowed value is checked first. Validate a borrowed view before creating
   an owned string; retain the allocation-free rejection path in `TryFrom<&str>`
   and avoid an unnecessary second allocation for an already-owned valid
   `String`. Add a regression test or an API-shape assertion that makes the
   ordering evident rather than testing only the eventual error variant.
2. `NoStatus` says any supplied `status` is refused, but the default permissive
   `Resource<M, S>` accepts an explicit `status: null` as `None`; only
   `DenyUnknown` currently refuses that spelling. Decide and document the
   intended generic rule, then test both permissive and strict no-status
   envelopes. The migrated catalog remains correct because it uses
   `DenyUnknown` and its non-null and null cases already pass.

After these are addressed, rerun the focused resource, Orishu wire/client,
catalog fingerprint, clippy, doc-test, and documentation checks recorded by
the task. The deliberately deferred `metadata.id`/`metadata.uid`, canonical
workload identity, and bounded workload-admission parser remain owned by their
named downstream work packages and do not block S-RESOURCE acceptance. (The
`metadata.id`/`metadata.uid` item has since been resolved — see "Still open,
deliberately" above.)

### Review resolution — 2026-09-07

Both findings are addressed.

1. **Discriminator bounds now precede allocation.** `ApiVersion::new` and
   `Kind::new` take `V: AsRef<str> + Into<String>` and validate `value.as_ref()`
   before calling `into`. An oversized or empty value is refused with no
   allocation; a valid `&str` is copied once, on the success path; a valid
   owned `String` is moved rather than copied a second time. A syntactically
   malformed value still allocates exactly one echoed copy, which the length
   check has already bounded to `MAX_LEN`.

   `crates/orishu-resource/tests/allocation.rs` asserts the ordering directly
   with a thread-local counting allocator, rather than asserting only the error
   variant — the previous ordering returned the identical `TooLong`. Restoring
   the old `into`-then-`check` body makes that test fail
   (`left: 1, right: 0`), so it is a real regression guard.

2. **The no-value field rule is decided, documented, and tested.** One rule
   governs both an unrecognised top-level field and an explicit `status: null`:
   the envelope's unknown-field policy. Permissive resources accept both;
   strict resources refuse both.

   The permissive half is a compatibility requirement rather than an
   oversight. Serde decodes `null` into an `Option` field as `None`, so the
   Orishu manifest this envelope replaced already accepted `status: null`;
   refusing it here would be a wire change hidden inside the extraction.
   `both_definitions_reject_and_accept_the_same_documents` pins that against a
   copy of the original derive.

   `NoStatus`'s rustdoc previously overclaimed and now states the actual
   matrix: a `status` carrying a value is refused under either policy, because
   it reaches the uninhabited status type; `status: null` follows the policy,
   because `Option` resolves null before the status type is consulted.
   Refusing every spelling needs `NoStatus` *and* the strict policy, which is
   the pairing `kagami-catalog` uses. All four combinations are tested, plus
   the same rule on a resource that does have a status type, and the rule is
   documented in [the shared resource envelope](../resource-envelope.md).

### Acceptance verification — 2026-09-07

The remediation was reviewed against the implementation rather than accepted
from its completion report. The discriminator constructor checks `AsRef<str>`
before converting with `Into<String>`; the allocation-counting integration
test proves rejection of oversized borrowed values allocates nothing and that
valid owned strings are moved without another allocation. The serialized
envelope tests cover valued and null status fields under both policies,
including a resource with a real status type.

Focused resource, Orishu wire/client, catalog/fingerprint, and Clippy checks
pass. The deliberately deferred identity and workload-admission items above
remain downstream work and do not keep S-RESOURCE open.
