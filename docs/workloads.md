# What is an Orishu workload?

An **Orishu workload** is the logical bundle of everything required to execute
one immutable simulation definition. Orishu can distribute that definition to
workers and evolve its initial conditions into simulation results. Formally,
the digest-linked contents of that logical bundle are called the workload
closure; they are independent of the physical format used to deliver them.

In compact form:

```text
workload = compute definition
         + initial conditions
         + digest-pinned executable components
         + required input artifacts
         + execution requirements
```

The workload is the input to a run. It is not the running cluster state and it
is not the results produced by that run.

## What a workload contains

| Part | Meaning |
| --- | --- |
| Manifest | The root definition: identity and metadata, domain and discretization, parameters, requested model, inputs, execution profile, and references to every required artifact. |
| Component graph | Bounded instances of digest-addressed WebAssembly Components plus their typed channels, deterministic step plan, ownership and placement constraints. Numerical kernels are implementation inside those components. |
| Initial conditions | The state at the initial simulation boundary: for example fields, composed particles and sources, geometry state, or a compatible checkpoint used to resume. |
| Other inputs | Immutable emitter spawn blueprints, geometry, meshes, material tables, accelerator data, schemas, or other artifacts required by this workload profile. |
| Requirements | The runtime lifecycle, numerical/determinism profile, hardware needs, resource limits, and compatibility rules workers must satisfy. |

The manifest is written in the
[shared resource envelope](resource-envelope.md) — `apiVersion: orishu.dev/v2`,
`kind: Workload` — which makes it familiar to read and inspect alongside every
other resource. Workload identity, however, is not generic resource metadata;
see [The schema](#the-schema) and
[Identity and provenance](#identity-and-provenance).

The manifest is declarative. It says which component instances must run, how
their typed phases compose, and with which inputs. The components supply the
scientific state transitions. Orishu supplies
the infrastructure around it: sandboxing, partitioning, halo exchange,
committed time, networking, checkpoint/result storage, and provenance. See the
[workload lifecycle contract](./protocol-workload.md).

The **compute definition** is the declarative part: domain, selected field
families and computational models, discretization, stepping policy, parameters,
and requested outputs. It is distinct from executable components. It declares
a bounded component-instance graph, typed state/contribution channels and a
deterministic step plan which Orishu validates and orchestrates. The complete
code graph is digest-pinned before submission and Orishu never chooses
executable physics implicitly from a domain or template name. See
[ADR 0024](./adr/0024-orishu-orchestrates-a-workload-component-graph.md).

## How the component graph advances one boundary

Each component instance owns or transforms only the typed state declared by
its model contract. For example, an electromagnetic component owns its field;
a coupling/projection phase consumes that field plus charged entity properties
and emits force contributions; Dynamics consumes admitted forces/impulses and
is the sole writer of candidate particle velocity and position. An emitter may
independently propose bounded new entities. Other plugins can introduce new
state and transformations through the same versioned channel/phase mechanism.

The step plan names these dependencies and deterministic reductions. Orishu
supplies committed inputs, isolates outputs, invokes ready nodes, performs
reliable transfers between differently placed producer/consumer partitions,
validates the assembled candidate and commits it atomically. Components cannot
call one another or share ambient memory. A failure in any required invocation
leaves the prior boundary authoritative.

Placement is runtime state, not workload identity. The manifest records only
scientific placement constraints—such as required accelerator capability,
compatible partition mapping, or mandatory co-location/separation. Orishu may
co-locate every instance for a small run or distribute field, projection and
entity work across eligible nodes without changing the component graph or its
scientific ordering.

## From simulation plugin to workload

A simulation plugin is an authoring-time package that combines declarative
Kagami schemas with digest-pinned workload component code. It makes a model available for
researchers to select and configure; it is not itself a running workload and
its installation location is not workload identity.

When Kagami compiles an experiment, it translates the selected plugin's model,
field, parameter, initial-condition, and observation choices into the workload
manifest and adds the exact required schemas and component artifacts to the
digest-addressed closure. Orishu sees only that immutable workload. It neither
consults Kagami's installed-plugin inventory nor resolves a mutable plugin name.

Gravity and electrodynamics shipped with Kagami follow this same path as
third-party plugins. Numerical kernels are implementation details used inside
workload components; object-catalog templates are reusable authored data and
cannot choose executable instances. See [Simulation plugins](./simulation-plugins.md).

Object behaviour remains explicit in the compiled component composition:
field-model instances own their field state, coupling/projection phases produce
typed entity contributions, and Dynamics alone integrates particle kinematics.
Orishu mediates the admitted step plan and may distribute independent component
partitions without changing workload identity or scientific ordering.
If an authored emitter selects catalog templates, its authoring command first
materializes their complete compositions into bounded blueprints in the
experiment. Kagami compilation revalidates and copies those blueprints into the
workload closure without consulting the current catalog.
Workers never resolve catalog paths or mutable template names. Spawned objects
are deterministic run state, and checkpoint state retains the emitter counters
needed to avoid loss or duplication across restart. See
[ADR 0020](./adr/0020-compose-object-behaviour-through-plugin-components.md)
and [ADR 0021](./adr/0021-capture-particle-emitter-recipes-in-workloads.md).

## From experiment to results

```text
editable Kagami experiment
          |
          | compile and pin every dependency
          v
immutable workload
  manifest + artifact closure
          |
          | distribute, validate, execute
          v
Orishu run
          |
          +--> observations
          +--> checkpoint artifacts
          +--> result artifacts
```

Kagami may compile one experiment revision into a workload. Once Orishu accepts
it, the workload is frozen for its workload epoch. Editing the experiment,
changing a variable, or selecting another kernel creates a new workload; it
does not modify the accepted one.

A checkpoint can be selected as the initial state of a later workload when its
state format and lifecycle are compatible. Results and observations are outputs
and never become inputs implicitly.

## Self-contained means a closed, pinned graph

A workload is **logically self-contained** when its manifest names every byte
needed for execution through an immutable artifact descriptor. Each descriptor
contains only identity-bearing information, including at least:

- the artifact's role;
- a cryptographic content digest;
- its byte size and media type;
- format/schema compatibility information where required.

Retrieval locations are distribution metadata, not workload content. A URL,
peer, local cache, submitting client, or future registry may all provide the
same blob. Such source hints live in the submission request, distribution
envelope, or live availability index and can change without changing workload
identity. A worker accepts bytes only after verifying the descriptor. Mutable
tags such as `latest`, unpinned URLs, local absolute paths, and “whatever this
server returns now” are not reproducible workload dependencies.

The **workload closure** is the root manifest plus every artifact reachable
from its descriptors. Two workloads may therefore share component artifacts or
underlying kernel blobs while using different graphs or initial conditions.
Workers that already hold those code blobs transfer only the new manifest and
missing input blobs.

## The schema

A workload is `apiVersion: orishu.dev/v2`, `kind: Workload`, in the
[shared resource envelope](resource-envelope.md). It is implemented by
`crates/orishu-workload`, which is deliberately the only definition: Kagami's
compilation and Orishu's admission will both consume these exact types, so no
second schema and no conversion between two models that could disagree ever
exists. Neither consumer is wired to it yet — that is the remaining
[shared workload-format work](tasks/define-and-adopt-shared-workload-format.md).

The `orishu.dev/v1` workload in `crates/orishu/src/model/workload.rs` is the
superseded prototype. It references code and inputs by mutable URI, so it cannot
express a closed pinned closure and has no stable identity. It is not migrated:
nothing has been submitted against it, so there is no compatibility obligation.
It remains readable only until worker admission moves to `v2`.

```yaml
apiVersion: orishu.dev/v2
kind: Workload
metadata:
  name: em cavity with charged particles   # discovery, not identity
  labels: {domain: electrodynamics}        # identity-bearing, unlike an annotation
spec:
  compute:
    workloadGraphProfile: orishu.workload-graph/v1
    components:                            # bounded sandboxed instances
      - instanceId: field
        artifact: <ArtifactDescriptor>     # role must be `component`
        pluginId: dev.orishu.electromagnetism   # provenance; never resolved
        modelId: dev.orishu.electromagnetism.yee/v1
        schemaId: dev.orishu.em.field/v1
        engine: wasm-component
        lifecycle: orishu.component/v1
        roles: [field-model]
        stateOwnership: [e-field]          # channels whose state it owns
        config: {permittivity: 8.8541878128e-12}   # resolved scalars
        limits: {maxMemoryBytes: 1073741824}
    channels:                              # typed state and contributions
      - channelId: e-field
        schema: {schemaId: dev.orishu.em.field/v1, version: 1}
        shape: [3]
        owner: field                       # absent for a contribution channel
        reduction: single                  # single | sum | min | max
    stepPlan:                              # the deterministic schedule
      profile: orishu.workload-graph/v1
      invocations:
        - invocationId: advance-field
          instance: field
          phaseId: update-field            # an admitted export, not any function
          inputs: []
          outputs: [e-field]
          dependsOn: []                    # the plan's edges; must be acyclic
    placementConstraints:                  # what is legal, never where it runs
      - constraint: co-locate
        instances: [field, dynamics]
  domain:
    dimensions: 3
    bounds: {shape: cube, sideMetres: 1.0}    # or {shape: box, sideMetres: [...]}
    discretization:
      spaceMetres: 0.001
      timeSeconds: 1.5e-11
      integration: {scheme: velocity-verlet, parameters: {substeps: 2}}
  inputs:
    geometry: <ArtifactDescriptor>
    initialConditions: [<ArtifactDescriptor>, ...]
    additional: [<ArtifactDescriptor>, ...]
  requirements:
    hardware: {minCpuCores: 4}
    executionProfile: {numericMode: deterministic}
```

An `ArtifactDescriptor` is:

```yaml
role: component            # component | initial-conditions | geometry | schema | ...
digest: sha256:<64 hex>    # algorithm-tagged; the only thing that names the bytes
sizeBytes: 20              # exact length, checked in addition to the digest
mediaType: application/wasm
schema: {schemaId: <string>, version: <uint>}   # where the role requires one
```

There is no `uri`, `path`, `registry`, `tag`, `peer`, or `credential` field, and
no inline-bytes variant. A document supplying one is **rejected**, not silently
stripped: every type refuses keys it does not recognise, because for an
identity-bearing document a dropped key would leave the author believing it did
something the digest says it did not.

Three things are structurally excluded from the manifest rather than merely
omitted: runtime status (the status slot's type is uninhabited), the
cluster-assigned resource `uid` and `namespace` (workload metadata has neither),
and retrieval locations.

Deferred to later work, and not yet in the schema: anisotropic and segmented
discretisation, unit-typed rather than canonical-SI quantities, and the
expression-bearing fields described under
[the client protocol](protocol-client.md). A manifest carries resolved
magnitudes until the variables integration lands.

## Canonical encoding and identity

A workload's identity is the SHA-256 digest of the **canonical encoding** of its
root manifest. Because every dependency in that manifest is named by digest,
that one value commits to the entire closure.

The canonical encoding is **deterministic CBOR** under
[RFC 8949 §4.2](https://www.rfc-editor.org/rfc/rfc8949#name-deterministically-encoded-c),
with this profile:

- definite lengths only — no indefinite or chunked arrays, maps, or strings;
- map keys sorted bytewise by their *encoded* form, with duplicates refused;
- shortest-form integer arguments;
- floats always 64 bits (the permitted "no float shrinking" variant), with NaN
  and infinities refused and `-0.0` normalised to `+0.0`;
- no tags, and no simple values other than `true` and `false`;
- an absent optional field is *omitted*, never encoded as null, and an empty
  collection is omitted rather than written out. "There are none" therefore has
  exactly one encoding: absence. An empty collection and an absent one are the
  same workload, and a document that writes one out explicitly is refused
  rather than accepted as a second spelling of it.

CBOR because it is already the canonical client payload format, so this adds no
second codec to the product. Both directions are written by hand rather than
derived from `serde`, so that a serialization attribute cannot silently relocate
what a workload commits to.

It is a codec, not only a hash input: canonical bytes decode back into a
manifest. That is what lets a receiver work from canonical bytes alone without
being handed the author's JSON, and it is also how the encoding is shown to be
*injective* — an encoder that quietly dropped a field would give two different
workloads one digest, and a round trip is what notices.

Decoding accepts a document **only when those bytes are exactly what encoding
the recovered manifest produces**. Every construct outside the profile is
refused where it occurs, with an error saying where and why; the result is then
re-encoded and compared against the input, which catches any second spelling not
already on that list. Both matter, because accepting a second spelling would
mean two byte strings decode to one workload — and the digest over the one that
does not re-encode would name a workload nobody could rebuild.

Encoding and decoding are bounded by the same manifest byte limit, and this is
symmetric on purpose: whatever encodes under a given bound decodes under it.
Encoding stops at the point it would exceed the budget rather than completing
and then being measured, so a manifest too large to read back is never given a
digest at all. An encoder allowed to outrun its own decoder would mint
identities for workloads nobody could ever load.

JSON and YAML remain how a person writes a workload. They parse into the typed
model and never define identity: whitespace, key order, comments, and the choice
of codec cannot change a workload's digest. Golden canonical bytes and digests
are checked in under `crates/orishu-workload/tests/fixtures/`.

## The authoring boundary

A workload document is hostile input even when it arrives from a trusted
operator, so reading one is a boundary rather than a convenience. Everything
untrusted enters through `orishu_workload::authoring` — `parse_str`,
`parse_bytes`, or `from_reader` — and each takes the caller's `Limits`
explicitly, because a network-facing admission path and a local file import
should not be obliged to accept the same sizes.

Bounds apply while the document is read, not after:

- the manifest's **byte length** is checked before parsing begins, so an
  oversized document is refused without being deserialized at all; and
- every **collection** stops at the point where accepting its next entry would
  exceed its bound. The entry that would have crossed it is refused without
  being deserialized, and a declared collection length is checked before it is
  reserved for rather than trusted. A document therefore cannot make a reader
  build a list in order to be told the list is too long.

Which bound governs which collection is recorded on the `Limits` field itself,
and the resulting error names the *collection* rather than the limit, so an
author is told which part of their document to shorten even where several
collections share one number. A refusal is a structured value
(`AuthoringError::CollectionTooLarge`), not only a sentence.

The model's types also have ordinary serde implementations, which is what makes
a manifest usable with any format and is the right tool for a manifest a caller
built or produced itself. Those are **not** bounded — a `Deserialize` impl
cannot see a runtime value — so untrusted bytes must not be handed to them
directly.

The same collection bounds are applied again to the whole manifest before
closure validation consults an artifact provider. That is not redundant: a
manifest may also be built by Kagami's compiler or recovered from canonical
bytes and never pass through the authoring reader, and the two ways of obtaining
a workload must admit the same workloads.

## Workload and distribution format are separate

The **workload** is the manifest and its complete logical closure. A
**distribution format** is one physical representation or protocol used to
move that workload. Different distribution formats can carry the same workload
when they resolve to the same root manifest and artifact digests.

The term **portable workload bundle** is reserved for a physical distribution
format that contains the complete closure. This is distinct from saying
informally that a workload is a logical bundle of required information.

The design provides at least two delivery forms for the same logical workload:

1. A **thin submission** sends the root manifest and then supplies only blobs
   the target cluster does not already have. Workers may retrieve missing blobs
   from the submitting client, verified peers, or optional external sources.
2. A **portable workload bundle** carries the manifest and its complete closure
   for offline export, archival, or one-file sharing. Importing the bundle adds
   its verified blobs to the same content-addressed store and then loads the
   same manifest.

The bundle is transport, not identity. Packing or compression must not create a
semantically different workload: after import, its manifest and artifact
digests are identical to those from a thin submission.

A whole-stream gzip or opaque monolithic package should not be the canonical
format. It forces unchanged kernels to be retransmitted with every input
change, prevents direct reuse of cached blobs, makes large-input range and
resume behavior awkward, and turns archive metadata/compression choices into
identity concerns. A portable archive may compress blobs independently, but
each stored representation remains bounded and digest-verified and large
scientific inputs may remain partition-aligned or chunked.

This model does **not** require a central artifact repository. A repository is
one optional source. The cluster's content-addressed cache and authenticated
peer transfer can distribute popular kernels once and reuse them across many
workloads, while a full bundle preserves the “copy one file and run it” user
experience.

ADR 0010 accepts this separation and allows multiple distribution formats. The
first portable archive encoding remains an implementation choice to be measured
against OCI Image Layout rather than part of workload identity. See
[ADR 0010](./adr/0010-content-addressed-workload-closure-and-portable-bundles.md).

## Identity and provenance

The root manifest has a canonical content digest — see
[Canonical encoding and identity](#canonical-encoding-and-identity) for how it
is computed. It commits to the artifact closure because every dependency is
named by digest. Signatures cover that root identity and therefore the pinned
graph; workers still verify every blob individually before use.

Verifying a closure is transport-neutral: a validator is handed the manifest and
something that can produce candidate bytes *by digest*, and it re-hashes
everything it receives. Nothing a provider says about a blob — its claimed role,
name, size, or origin — participates in the decision. Thin submission, a local
directory, a portable bundle, and an authenticated peer fetch are therefore
different sources for one verifier rather than four trust models.

Verification happens in two phases, and the order matters. Everything a manifest
can get wrong on its own — its bounds, its domain, its component graph, and
every claim it makes about its own artifacts, including contradictory
descriptors and both byte budgets — is decided from the manifest alone. Only a
manifest that passes reaches a provider at all: retrieval is work, possibly
network work, and a document earns it by being internally consistent first. A
contradiction is therefore reported even when the blob it concerns was never
supplied.

Candidates are streamed rather than held: bytes arrive in chunks, are hashed as
they go, and are never materialised by the validator. What a verified closure
records is *that* each artifact was verified and what the manifest said about
it, not its contents. This is what makes a 64 GiB artifact limit an honest
number, and it means a source supplying more bytes than declared is cut off at
the declared length rather than read to the end.

Run observations, checkpoints, and results record at least the workload/root
identity, workload epoch, executed component digest, input/state identity,
runtime lifecycle, and numerical execution profile. A human-readable workload
name is useful for discovery but is not an integrity identity.

## What is not a workload

- An editable Kagami experiment: it may still contain unresolved authoring
  choices and changes over time.
- A mutable URL or kernel tag without a pinned digest.
- A native library copied onto each worker outside the workload contract.
- A running simulation: that is a run of a specific accepted workload epoch.
- A result or observation: those are outputs produced by a run.
