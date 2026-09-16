# orishu-identity

The shared cluster-formation, node, and label identity contract. Two consumers
need the same types with incompatible dependency budgets: `orishu` carries an
HTTP client (`reqwest`, `tokio`, `chrono`), while the `orishu-membership`
functional core must build with no networking, async runtime, clock,
filesystem, TLS, or RNG dependency. This crate defines the types once and stays
dependency-poor so both can depend on it. See
[ADR 0013](../../docs/adr/0013-cluster-formation-and-node-identity.md).

## Public contract

Consumers use these types and never re-derive an identity from a raw string:

| Type | Role |
| --- | --- |
| `FormationId` | The generated ID of one ephemeral cluster formation. It scopes every post-admission peer message and durable provenance record. |
| `NodeId` | The unique ID a formation assigns each admitted member. Membership, ownership, administrative targeting, tombstones, and producing-node provenance use it. |
| `ClusterName`, `WorkerName` | Non-unique operator labels. They are deliberately not convertible to or comparable with an identity type, so a label cannot satisfy an identity check by accident. |
| `CertFingerprint` | A 32-byte certificate fingerprint. |
| `ProtocolVersion`, `ProtocolRange`, `VersionTuple`, `Incarnation` | Protocol version ordering. |
| `MembershipTombstone`, `RemovalMode` | Membership removal records. |
| `IdentityError` | The structured parse failure. Each variant names the offending field and the rule it broke, rather than a display string. |

## Wire representation

Every type serializes to the shape the peer and client protocols already
specify. Identities and labels are text strings. `CertFingerprint` is a
32-byte value — a hex string in JSON, a byte string in CBOR. `VersionTuple` is
the protocol's `VersionTuple` map. Deserialization is validating: a malformed
identity fails to parse rather than entering the domain as an unchecked
`String`.

## Testing and fuzzing

The identity types are the boundary check on hostile identity strings.

Contract testing: **has.** `tests/wire.rs` pins the serialized field contract
with golden JSON fixtures (re-bless with `ORISHU_IDENTITY_BLESS=1`) and asserts
the one place the encodings differ — a fingerprint is hex text in JSON and a
CBOR byte string. `tests/boundary.rs` drives the validating-deserialization path
over hostile input in both codecs, and `tests/dependencies.rs` enforces the
dependency budget that lets the sans-IO membership core depend on this crate.

Fuzzing: **has.** The `identity_types` target in [`fuzz/`](../../fuzz)
round-trips the validating deserialization of every identity, label,
fingerprint, version tuple, and tombstone.
