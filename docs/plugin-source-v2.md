# Local plugin source v2

Status: implemented source lowering and Unix package IO; no new installed-release,
bundle, workload or kernel ABI version. This is an input to `kagami plugin
validate/pack/inspect`, not a worker format or a code compiler. See
[local authoring tools](plugin-authoring-tools.md) and [ADR 0027](adr/0027-plugin-contributions-and-immutable-releases.md).

## Purpose and compatibility

Source v1 requires exact scientific contract references and artifact digests in
every declaration. V2 adds explicit source-local aliases so authors need not
manually hash local dependencies in topological order after every change. V1
remains accepted; aliases require `apiVersion: "orishu.plugin-source/v2"`.
The lowerer produces the same normal payloads and release root as equivalent
fully exact inputs. No alias, input path, source version or compilation ordering
enters the installed release. Contribution local IDs still belong to that release;
renaming those IDs can change release identity.

Plugins still build code externally. Packing neither launches a build tool nor
executes a guest, fetches dependencies or consults an installed inventory. An
independent plugin's contracts must be supplied as ordinary exact references;
local aliases never resolve a scientific display name or a mutable release tag.

## Manifest

The manifest shape is v1's, with the version changed and an optional `localId` on
explicit artifact entries. Contribution and artifact aliases use separate
namespaces and the existing `LocalContributionId` grammar (at most 64 bytes).
An artifact alias is not a path. Duplicate IDs in either namespace refuse the
package, even if their referenced bytes happen to be identical.

```json
{
  "apiVersion": "orishu.plugin-source/v2",
  "metadata": {
    "pluginId": "org.example.physics",
    "versionLabel": "development"
  },
  "contributions": [
    {"localId":"euler", "extensionPoint":"orishu.compute.integrators/v1", "path":"euler.json"},
    {"localId":"dynamics", "extensionPoint":"orishu.model.components/v1", "path":"dynamics.json"}
  ],
  "artifacts": [
    {"localId":"euler-code", "path":"euler.component.wasm", "mediaType":"application/wasm"}
  ]
}
```

Paths retain the existing secure bounded source-directory rules. Each artifact
is hashed from the actual bytes acquired for this package; no alias can point at
an undeclared file. Artifact aliases are optional when every reference is exact.
V1 manifests refuse artifact aliases.

## Declaration aliases

Known contribution JSON uses the ordinary scientific payload schema with only
these substitutions allowed:

| Location | Alias object | Result |
| --- | --- | --- |
| `scientific.requirements[].contract` | `{"localContribution":"dynamics"}` | Exact scientific name, version and digest of that local contribution |
| `scientific.kernel` on a field model or integrator | `{"localArtifact":"euler-code"}` | SHA-256 digest of that declared artifact |
| `presentation.iconArtifact` | `{"localArtifact":"icon"}` | SHA-256 digest of that declared artifact |

For example, the integrator declaration can contain:

```json
"requirements": [
  {"slot":"dynamics", "contract":{"localContribution":"dynamics"}}
],
"kernel": {"localArtifact":"euler-code"}
```

This is a fragment, not a complete integrator declaration. Its configuration,
execution contract, profile, history and other required fields still follow the
normal [payload schema](../crates/orishu-plugin/schema/plugin-v1.schema.json).
Exact and local requirements can coexist. An alias is a single-key object; mixing
an alias with an exact name/digest or extra fields refuses it. Other properties,
expressions and strings are never interpolated. Unknown extension-point payloads
remain opaque bytes, including anything resembling an alias inside them; they
cannot supply a local scientific contract.

## Lowering and refusal

1. Acquire bounded explicitly named artifacts and derive their digests.
2. Read bounded declarations, rejecting duplicate keys, nulls and invalid markers.
3. Compile ready known declarations using only already computed local contracts;
   forward references do not depend on manifest order. Hash each fully validated
   scientific projection after replacing its local dependencies.
4. Refuse a graph with no ready node: a dependency is absent, opaque or cyclic.
   Missing artifact aliases also refuse. No partial package is returned.
5. Validate all ordinary exact payloads and the complete release before packing.
   Normal safe publication refuses an existing output; failure never publishes a
   partially resolved release or mutates the inventory.

The pure `orishu_plugin::source::SourcePayload` reader/lowerer owns no IO. Kagami's
package adapter owns directory reads, the bounded local graph and final release
verification. Standard payload validation and identity projections remain the
only scientific interpretation; lowering is not proof of executable ABI admission.
Optional presentation aliases change full payload bytes but not scientific hashes.

Existing source caps apply: 1 MiB manifest, 256 contributions, 4096 explicit
artifacts, 256 KiB per known payload, 256 MiB per artifact and 1 GiB aggregate
acquired input bytes. Canonical release bytes have their own 1 GiB aggregate cap.
Tree depth, values, text and collection limits are enforced while reading each
known payload, including refusal before deserializing an excess list element.
Expansion is rechecked against normal payload limits; fitting a compact source
does not guarantee its exact lowered declaration fits. Ready-node scans have
worst-case O((V² + V×E) log V) work for V declarations and E local references, bounded by
the source caps. This cold packaging path makes no hot-loop or process-RSS claim.

Tests compare complete v1/v2 bundle bytes, reorder inputs, rename artifact aliases
and paths, propagate changed contract/code bytes, and refuse invalid local graphs.
The actual headless CLI validates and packs without creating an inventory, then
installs the resulting bundle through normal independent verification.
