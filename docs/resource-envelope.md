# The shared resource envelope

Orishu resources and Kagami object templates are read, printed, saved, and
inspected in one structural shape:

```yaml
apiVersion: <domain/version>
kind: <resource kind>
metadata: <resource-specific metadata>
spec: <resource-specific desired or descriptive data>
status: <optional resource-specific observed state>
```

`crates/orishu-resource` owns that shape and the `apiVersion`/`kind`
discriminator. Both `crates/orishu` and `crates/kagami-catalog` use it instead
of maintaining parallel definitions.

Related decisions:
[ADR 0008](adr/0008-catalog-templates-instantiate-self-contained-objects.md),
[ADR 0013](adr/0013-cluster-formation-and-node-identity.md).

## What sharing a shape does not mean

This is a library boundary, not a decision to behave like Kubernetes. The
shared envelope makes resources familiar to read and inspect. It does not
create an API server, a CRD mechanism, controllers, watches, generic
create/update/delete semantics, or a universal resource authority.

In particular:

- **There is no universal metadata type.** Orishu's `ObjectMeta` names a
  resource and optionally carries a system-assigned ID and a namespace.
  Kagami's `MetadataDocument` names a catalog and a template. Neither is a
  generalisation of the other, and `catalog` is not a namespace.
- **There is no universal identity.** Formation identity is `FormationId`,
  membership identity is `NodeId`, a template is identified by its catalog and
  name, and canonical workload identity is decided by the shared workload
  format. A generic string identifier never replaces one of these.
- **Labels and annotations are descriptive** unless a resource-specific
  decision explicitly makes them otherwise. They are not membership identity,
  they do not participate in formation identity, and they are not part of a
  workload digest.
- **JSON and YAML are human-facing codecs, not identity encodings.** Workload
  hashing uses the canonical codec the workload format selects; catalog
  fingerprints use catalog canonicalisation.

## Authority stays with the resource

| Resource | Authority and lifecycle |
| --- | --- |
| Workload | Immutable submitted definition plus a separately governed runtime projection. |
| Cluster | Synthetic runtime projection of current formation state. It is not a durable operator-authored `cluster.yaml`, and it has no create/update/delete lifecycle: `crates/orishu` gives it a constructor and deliberately no parser, so it can be rendered for a client but never read back as configuration. |
| Node | Cluster-owned membership projection. `metadata.name` is a reusable worker label. |
| Kagami object template | Client-owned editable catalog data. Its metadata, bounded YAML-stream loading, validation, canonical fingerprint, and write authority stay in `crates/kagami-catalog`. |
| Future experiment/plugin resources | May reuse the envelope only after defining their own authority, identity, versioning, and persistence contracts. |

## The discriminator

`apiVersion` and `kind` are validated new-types, bounded at construction. A
discriminator is read before anything else about a document is trusted, so it
is the one field a hostile input is guaranteed to reach; the bound is checked
against the borrowed string, before any copy is made.

`ResourceHeader` decodes the pair on its own and *permits* unknown fields, so a
document from a future format version is refused for its version rather than
for whichever unrecognisable field happens to be read first. `kagami-catalog`
uses this to distinguish "written for a newer Kagami" from "not a template at
all", which are different things for a user to act on.

The shared crate names no concrete version or kind. `orishu.dev/v1`,
`kagami.catalog/v1`, `Workload`, `Cluster`, `Node`, and `ObjectTemplate` are
their owning domain's constants, and mismatch errors report the *caller's*
expected pair alongside what the document carried.

## Fields that carry no value

The policy differs by consumer, and the difference is deliberate:

- **Orishu resources accept** top-level fields carrying no value.
- **Catalog documents refuse** them, so a misspelled key in a file a user
  edited is reported rather than silently dropped.

Both are expressed as a type parameter on the envelope rather than as two
definitions of the same shape.

One policy governs two spellings: a field the envelope does not recognise at
all, and an explicit `status: null`, which names a recognised field but
supplies nothing. Splitting them would let an authored document be strict about
a misspelled key yet lenient about a valueless one.

The permissive half is a compatibility requirement. Serde decodes a `null`
into an `Option` field as `None` everywhere, so the Orishu manifest this
envelope replaced already accepted `status: null`; tightening it here would be
a wire change smuggled into an extraction.

### What a resource with no status accepts

A resource whose status type is `NoStatus` — a Kagami object template, for
example — combines that with the policy:

| Document | `NoStatus` + permissive | `NoStatus` + strict |
| --- | --- | --- |
| no `status` key | absent | absent |
| `status: null` | absent | **refused** |
| `status: <value>` | **refused** | **refused** |

A value always reaches the status type, which has no inhabitant to decode
into, so it is refused under either policy. `null` never reaches it — `Option`
resolves null first — so it follows the policy instead. Refusing *every*
`status` spelling therefore needs both halves, which is why object templates
pair `NoStatus` with the strict policy.

## Bounds on the discriminator

`ApiVersion` and `Kind` are validated against the borrowed value before any
copy is made, on both the constructor and the deserialization path. This is a
hostile-input property rather than a micro optimisation: a discriminator is
read before anything else about a document is trusted, so it is the field an
oversized input reaches first, and copying before checking would let an
untrusted value allocate past `MAX_LEN` in order to be rejected. A counting
allocator in `crates/orishu-resource/tests/allocation.rs` asserts the ordering
directly, because the eventual error is identical either way.

## What each layer owns

| Concern | Owner |
| --- | --- |
| The five-field shape, field order, the discriminator | `orishu-resource` |
| Structural errors: empty, oversized, malformed, or mismatched discriminators | `orishu-resource` |
| Which `apiVersion`/`kind` values are supported | the resource's crate |
| Metadata schema, identity, and validation | the resource's crate |
| Spec and status validation | the resource's crate |
| Byte, nesting, and collection limits on input | the crate that owns the input |
| Multi-document streams, filesystem discovery, atomic writes, containment | the crate that owns the files |

Resource-domain validation errors stay structured in the domain that owns
them. `orishu-resource` never flattens a workload domain rule or a catalog
component diagnostic into a generic string.

## Dependencies

`orishu-resource` depends on `serde` and nothing else — no codec, no async
runtime, no transport, no filesystem, no UI, and no Orishu or Kagami domain
crate. Because both an Orishu crate and a Kagami crate depend on it, anything
it acquired, both would acquire. A test resolves the real dependency graph
rather than reading the import list, so a capability cannot arrive
transitively.

The crate has no parsing entry point of its own. Bounds on bytes, nesting, and
collection counts belong to the layer that owns the document: `kagami-catalog`
has one, and an Orishu network-facing path must supply one. There is
deliberately no convenient unbounded parser here to reach for instead.

## Where each format is defined

- Workload: [what is an Orishu workload?](workloads.md) and the
  [client protocol](protocol-client.md#workload-api).
- Cluster and node: the [client protocol](protocol-client.md).
- Object template: `crates/kagami-catalog` and
  [ADR 0008](adr/0008-catalog-templates-instantiate-self-contained-objects.md).
