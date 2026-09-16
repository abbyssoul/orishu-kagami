# orishu-resource

The shared typed resource envelope. Orishu's workload, cluster, and node
resources and Kagami's object templates are read, printed, saved, and inspected
in one human-readable shape:

```yaml
apiVersion: <domain/version>
kind: <resource kind>
metadata: <resource-specific metadata>
spec: <resource-specific desired or descriptive data>
status: <optional resource-specific observed state>
```

This crate owns that shape and its `apiVersion`/`kind` discriminator so the two
domains stop maintaining parallel definitions. See
[the resource envelope](../../docs/resource-envelope.md).

## Public contract

| Type | Role |
| --- | --- |
| `Resource` | The generic five-field envelope. Metadata, spec, and status are type parameters, so each domain keeps distinct types. |
| `ResourceHeader` | The discriminator alone, decodable before a body is trusted. A document from an unknown future version is refused for its version, not for whichever unrecognisable field is read first. |
| `ApiVersion`, `Kind` | Bounded, validated new-types. Each checks its `MAX_LEN` against the borrowed string before any copy is made. |
| `ResourceError`, `UnexpectedDiscriminator` | Structural failures only. Domain validation errors stay structured in the domain that owns them. |
| `AllowUnknown`, `DenyUnknown`, `NoStatus`, `UnknownFieldPolicy` | Compile-time policy for unknown fields and absent status. |

## What this crate is not

Sharing a shape is not sharing a lifecycle. There is no API server, no CRD
mechanism, no controller, no watch, and no generic create/update/delete
semantics. Metadata is a type parameter precisely so that a formation identity,
a node identity, a workload identity, and a catalog template identity stay
distinct types with their own authority and scope. Each domain keeps which
`apiVersion`/`kind` values it supports, its metadata schema and identity, and
its spec/status validation, canonicalisation, and persistence.

## Bounds and untrusted input

A discriminator is the one field a hostile document is guaranteed to reach,
because it is read before anything else is trusted. `ApiVersion` and `Kind`
check their bounds against the borrowed string, on the constructor and on the
deserialization path, so an oversized value is refused without allocation.

## Testing and fuzzing

Contract testing: **has.** `tests/envelope.rs` exercises the envelope and
discriminator contract, and `tests/allocation.rs` asserts the
reject-before-allocate ordering with a counting allocator. `tests/dependencies.rs`
enforces the `serde`-only dependency budget.

Fuzzing: **has.** The `resource_header` target in [`fuzz/`](../../fuzz) decodes
`ResourceHeader` in JSON and CBOR — the first-touch boundary on hostile
documents — and round-trips every successful decode, exercising the oversize and
unknown-version paths a finite unit suite cannot enumerate.
