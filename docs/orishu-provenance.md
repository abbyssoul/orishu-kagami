# Orishu output provenance

Orishu provenance ties durable checkpoints and results to the run, code, and
workers that produced them. It is distinct from workload-input trust and from
Kagami's authoring provenance.

## Authority layers

**Committed provenance** is stored in immutable artifact records and travels
with an artifact across cluster formations. It is authoritative for artifact
lineage.

**Diagnostic provenance** consists of step-provenance chains, audit events, and
logs. It helps reconstruct execution and operator activity but is transient or
best-effort and never overrides a committed artifact record.

## Committed fields

Checkpoint and result records retain, at minimum:

- workload identity and display-name snapshot;
- workload epoch and producing formation ID;
- producing-formation display-name snapshot;
- exact artifact and per-chunk content hashes;
- relevant simulation boundary or time range;
- executed component digest, lifecycle, and execution profile;
- schema/model, dimensions, precision, validity, and completeness metadata; and
- producing worker/runtime identities required by the accepted profile.

Display names and wall-clock timestamps are diagnostic. Formation, workload,
epoch, boundary, component, and content identities carry authority.

## Chain

```text
workload closure + accepted execution profile
    -> run identity (formation, workload, epoch)
        -> committed simulation boundary
            -> immutable checkpoint/result artifact
                -> verified partition chunks and producing runtime evidence
```

A checkpoint records sufficient identity to prevent use with an unrelated or
incompatible workload. A derived result sequence groups artifacts but adds no
new authority.

`StepProvenance` records which worker/runtime/component produced a partition and
step range. It is useful forensic evidence and may be carried with checkpoint
coordination, but durable output must summarize or anchor the provenance it
needs rather than depend on a transient gossip history remaining available.

Audit events record who performed operations such as workload start or artifact
purge. They are provenance for administrative actions, not proof of artifact
content or exclusive ownership of cluster state.
