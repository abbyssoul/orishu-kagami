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
         + pinned compute kernel
         + required input artifacts
         + execution requirements
```

The workload is the input to a run. It is not the running cluster state and it
is not the results produced by that run.

## What a workload contains

| Part | Meaning |
| --- | --- |
| Manifest | The root definition: identity and metadata, domain and discretization, parameters, requested model, inputs, execution profile, and references to every required artifact. |
| Workload component | The content-addressed WebAssembly Component implementing the governing equations through Orishu's sandboxed workload lifecycle. This is the compute kernel executed by workers. Older documents may call it the workload package; it is not a distribution bundle. |
| Initial conditions | The state at the initial simulation boundary: for example fields, particles, sources, geometry state, or a compatible checkpoint used to resume. |
| Other inputs | Immutable geometry, meshes, material tables, accelerator data, schemas, or other artifacts required by this workload profile. |
| Requirements | The runtime lifecycle, numerical/determinism profile, hardware needs, resource limits, and compatibility rules workers must satisfy. |

The manifest is declarative. It says what must be run and with which inputs;
the workload component supplies the executable state transition. Orishu supplies
the infrastructure around it: sandboxing, partitioning, halo exchange,
committed time, networking, checkpoint/result storage, and provenance. See the
[workload lifecycle contract](./protocol-workload.md).

The **compute definition** is the declarative part: domain, fields and physical
model selection, discretization, stepping policy, parameters, and requested
outputs. It is distinct from the executable component. The initial profile
loads one root lifecycle component for a workload. That component may be built
from reusable numerical kernels or composed components, but the complete code
dependency graph is pinned before submission; Orishu never chooses executable
physics implicitly from a domain or template name.

## From simulation plugin to workload

A simulation plugin is an authoring-time package that combines declarative
Kagami schemas with a pinned workload component. It makes a model available for
researchers to select and configure; it is not itself a running workload and
its installation location is not workload identity.

When Kagami compiles an experiment, it translates the selected plugin's model,
field, parameter, initial-condition, and observation choices into the workload
manifest and adds the exact required schemas and workload component to the
digest-addressed closure. Orishu sees only that immutable workload. It neither
consults Kagami's installed-plugin inventory nor resolves a mutable plugin name.

Gravity and electrodynamics shipped with Kagami follow this same path as
third-party plugins. Numerical kernels are implementation details used to build
the workload component; object-catalog templates are reusable authored data and
cannot choose the component. See [Simulation plugins](./simulation-plugins.md).

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
from its descriptors. Two workloads may therefore share the same workload
component or underlying kernel blobs while using different initial conditions.
Workers that already hold those code blobs transfer only the new manifest and
missing input blobs.

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

The root manifest has a canonical content digest. It commits to the artifact
closure because every dependency is named by digest. Signatures cover that
root identity and therefore the pinned graph; workers still verify every blob
individually before use.

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
