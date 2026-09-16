# Selected plugin closure

Status: **pure compilation and independent declaration/byte verification
implemented, including fixed workload-profile/runtime admission; application
integration pending**.
This implements the evidence portion of [X-PLUGIN R6](plugin-contract-v1-draft.md#8-selected-workload-closure-and-provenance-r6),
not another workload manifest or proof that a kernel is safe to execute.

`orishu_plugin::selected` owns the shared descriptor and validation. It depends
on the existing workload identity/CBOR primitives, never on a Kagami inventory,
worker, filesystem, network, Wasm engine or mutable plugin registry. A worker
receives exact selected bytes, not an instruction to install a plugin.

## Identity-bearing descriptor

`orishu.plugin-selection/v1` uses the existing deterministic CBOR profile and an
explicit projection. Its fields are:

- `apiVersion`: exactly that schema discriminator.
- `roots`: explicitly used, provider-qualified scientific contributions.
- `contributions`: the exact reachable transitive contribution set.
- `bindings`: `{consumer, requirementSlot, provider, exactContract}` for every
  selected requirement, including dependencies local to one release.
- `kernelInstances`: `{instanceId, contribution, executionContract}` for each
  configured executable use. Every selected executable must have a use; multiple
  instances of one selected kernel are permitted.
- `releaseEvidence`: `{release, artifact}` for exactly the selected releases.
  Each artifact is the complete canonical root with media type
  `application/vnd.orishu.plugin-release+cbor`.

Contribution sets sort by full `ContributionRef`; bindings by consumer then slot;
instances by instance ID; evidence by release identity. Readers reject duplicates,
noncanonical order, unsupported versions and unknown fields. `roots` makes an
unreachable extra contribution detectable; it does not independently prove that
the enclosing experiment actually uses that root. Workload integration must check
that relationship against captured objects, fields, configuration and history.

Release identity and root artifact digest cover the same canonical bytes but
remain distinct typed identity domains. Evidence proves membership, not publisher
authenticity. As with ordinary release files, metadata must not contain secrets.

## Compilation and verification

`selected::compile` accepts exact resolver choices, explicit kernel instance uses,
verified release handles and borrowed source blobs. It produces canonical
descriptor bytes, newly generated canonical root evidence, and the unique required
artifact descriptors. Kernel/payload bytes stay in the caller's store; the compiler
does not copy a whole installed bundle. It revalidates the result through
`selected::verify`, rather than treating a serialized resolver result or an earlier
release check as admission authority.

`selected::verify` independently:

1. Checks descriptor bounds, order, references and exact release-evidence coverage.
2. Verifies canonical root bytes, sizes and release identities.
3. Verifies selected membership, payload digests and scientific declarations.
4. Checks both ends of every exact dependency edge, prohibits rebinding a local
   dependency to a different provider, and rejects missing/extra edges, cycles
   and unreachable contributions.
5. Checks field-family, observable, field-coupling and Dynamics provider roles,
   including a model's coverage of the family's required observable contracts.
6. Checks each configured executable role and the one-contract-per-artifact rule
   across selected releases, then verifies all required artifact bytes.

Required artifacts are **root evidence + selected payloads + selected kernels**.
An evidence root's artifact list is historical metadata, not a recursive dependency
list. Unselected code, icons, documentation and opaque future contribution payloads
are neither fetched nor parsed. Extra caller-cache bytes are ignored. Unknown
*selected* extension points fail closed. Conflicting descriptors for one required
digest are refused.

The result is an immutable `VerifiedSelection`, containing small declarations and
required artifact descriptors, not executable instances. Its artifact list does
not include the descriptor itself or the workload's scientific input/state blobs;
the enclosing workload exporter must include and bind those too.

## Bounds and evidence

Default policy permits a 1 MiB descriptor, 4,096 contributions, 16,384 bindings,
256 kernel instances, 256 release roots, 8,192 unique required artifacts, 1 GiB
required bytes and 32 MiB aggregate parsed root/payload bytes. Per-declaration
limits also apply. Metadata is charged before parsing each next root/payload;
required artifact count/bytes are checked before hashing executable blobs. The
canonical reader bounds its intermediate tree before typed decoding. Graph walks
are iterative O(contributions + bindings); schema checks and release-list lookup
also have explicit caller-owned list/byte bounds. No provider candidate search
occurs at this boundary.

Tests cover independent vocabulary/solver releases, exclusion of unused kernels
and malformed opaque payloads, canonical round trips and all truncated prefixes,
missing/corrupt/wrong-size required bytes, local-vs-external exact binding behavior,
provider-role/channel incompatibility, selected executable roles, unsupported
selected points, raw/wire budgets and compiler revalidation of changed source
bytes. These fixtures deliberately use inert kernel bytes: actual Wasm execution
is separately exercised by the [runtime tests](runtime-fixed-run.md).

## Integration still required

Existing workload-v2 bytes and public types are unchanged. The
[v3 root and captured execution definition](workload-v3.md) now provide the explicit
versioned seam, with cross-version rejection tests. `selected::verify_context`
checks declaration-derived context semantics against selected uses. The shared
`workload` compiler/verifier now binds the exact graph, selected closure and captured
inputs to the fixed scientific profile. `orishu_runtime::admit` adds actual Wasm
ABI, configuration/state validation, code budgets and a caller-owned digest deny
policy before constructing a run. Runtime admission tests use real independently
verified releases and actual reference Components; the pure selection tests above
continue to use inert code intentionally.

Kagami still needs document-format adoption and selected export. Orishu still needs
artifact delivery, fenced run allocation and endpoint adoption of shared admission.
This library result must not be reported as an end-to-end runnable workload.
