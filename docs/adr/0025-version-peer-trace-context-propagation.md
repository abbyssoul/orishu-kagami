# 0025 — Version peer trace-context propagation separately from membership semantics

Status: **proposed — review required before changing the negotiated profile**

## Context

[ADR 0017](0017-worker-operational-observability.md) requires sampled
client-to-peer tracing without making telemetry an authority or core dependency.
The current `orishu-membership/4` wire adapter requires seven envelope fields
and rejects unknown ones. Adding optional context is therefore incompatible
with existing peers, even when its sender considers the field optional.
Silently relaxing profile 4 would make telemetry enablement change interoperability.

## Proposed decision

Introduce `orishu-membership/5` when bounded propagation and its compatibility
fixtures are ready. Keep semantic `proto: 1`, policy Merkle hashing, admission
attempt identity and assignment replay unchanged. Do not negotiate profile 4
as a fallback. As with previous PoC profile changes, rebuild/restart all workers
together; rolling upgrades and retained formation recovery remain unsupported.
Until accepted and implemented, the supported profile remains 4.

All profile-5 builds understand the optional membership-envelope `traceParent`
field, independently of exporter features. Exporter dependencies stay behind
`otlp-tracing`. Disabled tracing emits no context and performs no export;
receivers may discard context without changing the domain request. Handshake
and admission-baseline DTOs retain their existing shapes in this increment.

Use a bounded subset of [W3C Trace Context](https://www.w3.org/TR/trace-context/):
nonzero 16-byte trace and 8-byte parent IDs, lowercase hexadecimal, version-00
output, and only the sampled flag on output. Incoming sampling is advisory,
never permission to bypass local sampling or queue budgets. No `tracestate` or
baggage is retained or forwarded; this is not transparent vendor-context forwarding.
Exact parsing and failure rules belong in the client/peer protocols.

Context stays in IO-owned request/effect metadata, never `PeerInput`, core
effects, replayable membership state, operation identity or admission digests.
An asynchronous owner job retains at most one fixed parent context, fenced by
its existing operation/generation. Retries can create new spans but cannot
change operation identity or restart a retry budget. Unrelated periodic work
does not inherit the most recently received client's parent. Multi-cause gossip
must use bounded links or a fresh trace, not invented single-parent causality.

Telemetry may not make a valid packet too large. Encode/select the ordinary
domain packet first; add context only if it fits the unchanged stream/datagram
limit. Otherwise omit context without changing payload, extra gossip selection,
delivery class or domain success. A truncated trace is preferable to altered
membership behavior. Context bytes count toward all existing transport limits.

## Alternatives

- Extend profile 4 silently: rejected; existing decoders reject the field.
- Negotiate both profiles and maintain two encoders: deferred; adds downgrade
  and capability state while rolling PoC upgrades remain out of scope.
- Correlate local spans by operation ID alone: useful diagnostics, but does not
  satisfy received cross-peer trace-context evidence.
- Put context in core messages or membership hashes: rejected; changes replay,
  authority and identity for optional best-effort diagnostics.

## Consequences and acceptance

This requires a coordinated PoC restart, not a persisted-data migration. Metrics
and probes remain independently usable. Feature-disabled peers still implement
one wire grammar. No context field or profile bump is implemented by this ADR.

Before activation, verify old-profile refusal with valid mutual credentials;
golden absent/present-context bytes; invalid-context versus invalid-domain
behavior; unchanged admission digests/replay; maximum packet omission; and all
feature combinations. Then verify a real client operation and peer operation
reach the test OTLP receiver with the documented parent relationship. Codec
fixtures alone do not establish propagation or export.

Delivery: [observability task](../tasks/implement-worker-observability.md),
[formation task](../tasks/implement-cluster-formation-poc.md),
[peer protocol](../protocol-p2p.md#proposed-trace-context-extension),
[client protocol](../protocol-client.md#planned-client-trace-context).
