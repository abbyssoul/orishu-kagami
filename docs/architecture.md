# Architecture

Orishu Kagami separates editable intent, distributed execution, and
presentation without treating them as separate products.

```text
Kagami authoring and presentation
        |
        | compile experiment / send client commands
        v
Orishu client interface and workload contract
        |
        | authenticated control + subscribed observations
        v
Orishu cluster (one workload, many nodes)
        |
        | committed simulation boundaries
        v
result and checkpoint artifacts
```

## Modules

### Deployable applications

- `apps/orishu-worker` runs a node and owns cluster/runtime behaviour.
- `apps/orishu-ctl` provides scriptable operator administration.
- `apps/orishu-monitor` is the interactive terminal operator client.
- `apps/kagami` is the native experiment-authoring and visualization client.

Applications compose library modules; other applications do not depend on an
application crate.

### Shared libraries

- `libs/orishu` owns shared cluster/workload models and the Orishu client
  interface. It is the seam used by every operator or visualization client.
- `libs/kagami-renderer` hides Kagami's GPU pipeline behind Iced's shader-program
  interface. It renders presentation state and must not own simulation state.

The prototype scene tree currently lives in `apps/kagami` because it is demo UI
state, not the authoritative experiment model. Extracting it as a library would
create a shallow module and prematurely bless the wrong model.

## Kagami and Orishu

Before submission, Kagami owns an editable experiment. Submission will compile
a supported profile into an immutable Orishu workload manifest, package
reference, and input artifacts. After acceptance, Orishu is authoritative.

Kagami communicates only through the authenticated client interface. It does
not join the peer network, own partitions, commit simulation steps, or directly
access worker GPU buffers. Live visualization consumes complete, versioned
observations associated with committed simulation boundaries.

The long-term local-preview adapter and remote-Orishu adapter must satisfy one
observation interface. Two adapters make that seam real; until both exist, the
repository should avoid inventing abstractions around hypothetical variation.

## Trust and data flow

All network payloads are untrusted. Control messages and streamed observation
chunks require size limits, checked offsets, schema/version validation,
integrity checks, and bounded buffering. Kagami may retain the last complete
observation while disconnected, but it must label it stale and must never mix
chunks from different observation identities.
