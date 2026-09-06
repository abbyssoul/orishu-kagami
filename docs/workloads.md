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
