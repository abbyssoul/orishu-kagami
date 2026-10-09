# Experiment container v5 — standalone component provider intent

Status: implemented document/session core and durable save/open path under
[X-PLUGIN](tasks/define-and-implement-plugin-contract.md). [Catalog v3](catalog-template-v3.md)
now materializes scoped locks atomically. The [Unix existing-component form](component-provider-choices.md)
also saves guarded provider choices. Native removal explicitly prunes locks in the
same edit batch. Unavailable first attachment and new locked-document roots now
use guarded schema/component/lock adoption; native exact Add captures choices in
previously unlocked uncaptured documents too. Explicit uncaptured replacement
accepts detach/new values/scoped lock atomically. Ordinary native captured Add
now uses consented atomic component/lock/evidence/history adoption, retaining
fields. Captured replacement has a full-reset backend using this same format;
native selected-component/binding-only reset uses two-stage input-specific consent.
Guarded compound/catalog addition preparation now persists copied definitions,
complete locks and regenerated history through the same format. Native catalog
creation now supplies instance inputs, provider choices and captured-history
consent. General composition/migration planning and MCP/headless adapters remain.

## Authority and compatibility

An experiment may retain a complete [component dependency lock](plugin-contract-v1-draft.md#standalone-authoring-dependency-lock)
without a field, solver or scientific initialization. `AdoptDependencies` is an
atomic document command. It may accompany component creation/removal/replacement
in one batch; final lock roots must exactly equal the set of provider-qualified
components attached to objects. Legacy logical references remain unresolved and
never acquire an inferred provider. An absent lock preserves existing legacy
authoring behavior, not a claim that dependencies have been resolved.

Once a lock exists, changing the exact component root set requires an updated
lock in the same batch. There is no implicit pruning, replacement, or dropping of
provider choices in the authority. Catalog v3 instantiation constructs combined
object/lock edits; native removal explicitly submits the surviving closure with
the removal (and replacement history if required). New locked-document roots
use guarded dependency resolution and atomic schema/component/lock adoption.
Uncaptured replacement explicitly restricts surviving intent and resolves the new
root before atomic detach/attach/lock acceptance. Shared providers remain pinned;
old property values are not implicitly converted. Captured component attachment
now uses an input-specific consented proposal and guarded scientific extension;
explicit provider replacement has a separate full-reset backend whose result
uses the same container format, now with a native two-stage consent workflow.
Compound/catalog additions and general multi-object migration adapters remain open.
Existing-component choices have a Unix
adoption form without physics initialization. Internal lock adoption is not
available through the existing ordinary JSON command DTO. Inventory resolution
and a final revision/availability guard remain adapter duties before adopting a
new choice. Offline hydration preserves unavailable intent without an inventory.

If the draft also has captured scientific setup, its independently verified
selection must contain every locked member and exactly the same dependency edges
for those members. A lock cannot omit a required edge or override an execution
provider. Other field/integrator selections may extend the overall closure. This
is one authoring intent checked against execution evidence, not two authorities.
Scene compilation repeats the agreement check before emitting workload inputs.

The lock participates in normal revision, dirty, undo/redo and atomic-rejection
semantics. Its canonical byte weight is cached during construction and charged
once per shared allocation in the authority's existing retained-state budget,
across current state, history and accepted request receipts. This is a logical
metadata weight, not total RSS. Exact dependency-only releases also participate in
bounded reference queries and existing window lease reconciliation. Open rechecks
the receiving authority's tighter per-lock and aggregate retention policies.

## Flat file layout

The durable writer uses a stored-ZIP v5 container only when standalone intent is
present. Unlocked legacy drafts still write JSON v3; unlocked scientific drafts
still write [container v4](experiment-container-v4.md), with unchanged bytes.
Opening v1–v4 never invents a lock. Bare legacy JSON serialization of a locked
document refuses rather than discarding intent. Older readers decline v5.

The archive has one `document.json` plus digest-addressed blobs, using the same
strict archive framing as v4. There is no nested document/container. Its root is:

```text
{
  format: "kagami.experiment",
  formatVersion: 5,
  metadata: <existing DocumentMetadata>,
  dependencies: {digest: <sha256>, byteLength: <u64>},
  experiment: {
    counters: <existing identity counters>,
    setup: {kind: "legacy" | "scientific", value: <matching setup>},
    variables: <existing authored definitions>,
    objects: <existing authored objects>
  },
  defaultView?: <existing independently versioned view>
}
```

`dependencies` is required and references the exact canonical
`orishu.plugin-authoring-lock/v1` blob. The legacy setup value is the existing
domain/timestep/logical-plugin shape. Scientific setup uses v4's
`kagami.scientific-setup/v1` description and the same declaration/configuration/
state/history blobs. Provider-only drafts need only their lock blob; they carry
no executable or synthesized scientific state. The union of required blobs must
be exact: missing, corrupt, length-mismatched or unrelated blobs refuse.

`ContainerLimits.dependencies` independently bounds lock bytes/graph work/counts/
depth. Existing archive and metadata limits also apply. Lock length is checked
before decoding its graph; scientific blob aggregate admission precedes copying
captured state. Scientific buffers remain shared during encoding. The decoder
normalizes to the existing in-memory document version, then `into_experiment`
validates object roots, expressions and captured-selection agreement. A decoded
DTO alone is not authority acceptance. Unsupported versions and policy refusals
do not trigger damage recovery around an intact newer file.

The document read projection is now **model wire v4**, adding optional
`dependencies: <LockDescription>`. That description is raw serializable data,
not a validated lock or executable permission. Existing workload/release/kernel
identities and worker protocols do not change.

## Evidence and remaining integration

Session tests exercise combined edits, undo/redo, idempotent receipts,
dependency-only reference retention, atomic budget rejection, receiving-policy
checks, offline v5 round trips, hostile metadata/blobs and durable write-failure
windows. The real Kagami fixture now composes two bodies from a chosen non-default
vocabulary, saves their nonempty component lock through the window controller,
initializes Newtonian/Euler physics, reopens offline, exports unchanged selected
evidence and executes through fresh inventory-free runtime admission. Both bodies
acquire attractive velocities. Catalog-v3 tests separately cover scoped
materialization/conflicts and safe writes. V3 JSON/v4 container golden tests remain.

Compound/catalog captured-addition adapters, general multi-object replacement
beyond the native selected-component/binding-only reset, and MCP parity
remain required before closing this authoring slice. They must preserve complete
locks and guards/reference lifetimes.
