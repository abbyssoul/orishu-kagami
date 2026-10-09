# Component-only provider choices

Status: implemented Unix window workflows for **existing-component choices,
staged attachment before/after scientific capture, uncaptured replacement and
captured replacement/binding-only full reset**. This is part of
[X-PLUGIN](tasks/define-and-implement-plugin-contract.md), not its completion.
The guarded full-reset backend and native two-stage consent workflow are connected;
The guarded backend also prepares captured compound/catalog additions. The native
catalog picker now creates self-contained objects before/after capture, with
explicit provider choices and history consent. MCP/headless adapters remain separate.

## Researcher workflow

Select **Scientific setup / fields** in the scene inspector, then:

1. **Load current component choices** copies exact component roots and any saved
   bindings into a local proposal. It does not modify the document or require
   selecting a field, integrator, domain or timestep. Legacy logical components
   are left unresolved rather than guessed.
2. **Check dependencies** resolves the proposal without executing kernels.
   Ambiguities show bounded candidate pages. **Browse installed alternatives**
   includes compatible enabled non-default releases without changing defaults.
3. Choose explicit providers and check again. If changing an upstream provider
   leaves bindings for an unreachable old consumer, **Discard unused local
   binding** removes only a binding identified as unused by the current report.
   It is an explicit proposal edit, not silent pruning of saved intent. Clearing
   local choices restores the copied saved bindings, not installation defaults.
4. **Save component choices (undoable)** independently revalidates the displayed
   complete graph and submits one guarded `AdoptDependencies` command. No fields,
   scientific buffers, JIT engine or guest instances are created. The document
   becomes dirty and the choice participates in normal undo/redo and
   [container v5](experiment-container-v5.md) save/open.

Loading, browsing and choosing are not acceptance. After an edit, reopen, mode
change or inventory refresh, reload/recheck explicitly. Old report tokens or
candidate indices cannot select from a different page. No automatic retry occurs.
For a captured experiment, loading component choices instead stages the full-reset
workflow below. It cannot save changed bindings against unchanged scientific state.

## Shared preparation and authority boundary

`PluginStore::prepare_authoring_lock` takes the exact closed lock, expected
inventory revision, process-only overrides and caller-owned lock limits. It
revalidates all members and bindings with `Inventory::resolve_lock`, projects
selected component schemas from the same verified declarations and retains exact
release leases. Missing, disabled and incompatible providers remain structured
unavailability; an omitted required edge cannot be silently filled from defaults.
Package verification uses the existing bounded inventory loader, which can read
package artifacts; it is not a claim of zero artifact IO or lazy kernel loading.

The window uses the same bounded discovery/page controller as the physics form,
with separate proposal generations. The preparation runs in its retained worker
slot. Its final inventory-revision guard remains held through the document's
incarnation/revision/mode check and atomic command acceptance. Release leases
cover adoption into document reference tracking. `DocumentAuthority::submit_with_schemas`
stages verified capabilities, the normal command, events, receipts and history
together. A refused or stale completion changes none of them; replay returns the
original receipt without adopting a new projection. On acceptance, selected
schemas restore previously unavailable authoring capabilities. That projection
is not persisted intent, another scientific revision or a provider-selection rule.

New whole-physics proposals include saved component bindings. A conflicting
physics-form binding refuses before initialization rather than silently overriding
the document lock. Captured evidence and workload compilation still independently
check agreement with that lock.

## Adding a component with dependency resolution

Startup and explicit inventory refresh now retain bounded unresolved component
pins, not just their count. The object inspector offers **Resolve and add** for
enabled declarations whose dependencies are unavailable or ambiguous. Adding an
exact component that introduces a new root uses this same proposal path, even
when its schema is already available and the document has no lock yet. Thus
ordinary first attachment saves the complete resolved choices rather than leaving
them to a later installation's defaults. Legacy logical component actions are
unchanged; existing unlocked files/templates are not silently migrated on read.

The selected object's inspector shows the proposed component, Check/Browse/Choose
and **Attach component and save choices**. Nothing is attached while choosing.
The proposal copies all existing roots and bindings, adds the requested exact
root, and refuses attempts to rebind or discard accepted edges. Final preparation
independently verifies the complete graph; merging with existing intent must
preserve every common consumer's complete bindings. Declared defaults or explicitly
entered values, attachment and lock are accepted in one revision using the staged
schema path. Missing required values still refuse; neither an incomplete component nor its
prepared schema leaks into the live authority. Undo removes component and lock
together; installed capabilities remain a separate projection, not history.

Cancel discards the local proposal. An in-flight read retains its worker slot
until completion, but cannot accept against the cancelled generation. Target
removal, another edit, document replacement, mode changes and inventory changes
invalidate adoption through the existing guards. A new component choice never
authorizes migrating already captured scientific state.

Every uncaptured exact attachment offers a proposal, including another use of an
already locked root: the second object can need different values or have required
fields without defaults. An unchanged exact lock reuses the existing allocation.
An unlocked uncaptured document resolves all its exact roots when a native exact
Add is accepted, without substituting for legacy logical IDs. Captured additions
use the consented extension workflow below; provider replacement uses full reset.

### Entering attachment properties

Available exact schemas immediately show local property fields and their declared
defaults. For an unavailable schema, first resolve its dependencies, then choose
**Load property fields (no document change)**. The same guarded verified-selection
read supplies the declaration, but does not install a schema or attach a component.
Its temporary release leases cover that read; Apply independently revalidates
availability again. Repeated field inspection never resets already entered values.

Quantity fields retain expression source, text fields retain literal UTF-8, and
boolean controls distinguish `false` from absence. **Unset** removes a proposed
value rather than inserting a default; **Set empty text** authors an explicit
empty string. Requiredness, dimensionality, expressions and plugin constraints
remain authority checks on Save. Failed acceptance retains the local values for
correction while leaving document, capabilities, history and receipts unchanged.
Without a schema on offline reopen, quantities preserve source but are unevaluated.

Property messages carry their own form identity, so an old field cannot edit a
new attachment. Accepted local changes invalidate pending Apply/resolution work,
requiring Check again. Form storage caps fields at 256, applies the document's
property-count/expression/text bounds and caps combined source bytes at 1 MiB;
defaults are checked before cloning. This is bounded local proposal storage, not
a whole-process allocation claim. Final admission compares the displayed exact
declaration with independently prepared evidence and uses the normal atomic
schema/component/lock path; fields are never executable authority.

## Explicit component replacement

On an existing object's component, choose **Choose replacement…**, then select a
different exact component, configure its values and check dependencies. **Replace
component and save choices** accepts detach, attachment and revised lock in one
guarded revision. The old component remains untouched until acceptance. Cancelling
or a failed/stale proposal leaves it intact. Successful acceptance ends the chooser.

Replacement starts from the new declaration's defaults and explicit local values;
matching property names do not imply conversion or copying from the old component.
It can replace a legacy logical component only through this explicit choice, never
through an inferred provider mapping. No additional component slot is required:
detachment precedes attachment inside the atomic batch.

The proposal restricts saved intent to surviving roots before resolving the new
root. Other uses of the old component preserve its root and dependencies. If the
new root was already a selected dependency, its complete existing choices remain
pinned as well. Shared consumers cannot be rebound; only intent made unreachable
by the explicit replacement is pruned. Independent final preparation revalidates
the graph, schemas and availability using the same guards and leases as attachment.
Undo/redo restore old/new components, values and locks together; offline save/open
preserves the accepted intent. No field initialization or kernel execution occurs.

For a captured setup, this chooser instead stages the coordinated full reset below.
Uncaptured replacement never authorizes discarding scientific state implicitly.

### Explicit captured replacement: backend boundary

`ScientificEffects::reset_components` now accepts a `ComponentResetRequest` with
explicit scene edits, a complete resulting component lock and a complete new
`SetupRequest` (models, providers, domain, configuration and timestep). Its caller
must obtain consent to discard **all** old field/history state. This is a full
reset, not automatic value conversion, selective state migration or ordinary Add.
New component values and all provider changes must be supplied explicitly.

The background operation independently verifies the new authoring lock and
scientific selection. Shared `prepare_scientific_reset` folds the same ordinary
commands and validates schemas, expressions, lock roots and agreement with the
new verified evidence. It exposes only variables and projected Dynamics input,
not an adoptable snapshot or old opaque state. Old capture coherence is deferred
only inside this preparation; ordinary acceptance still checks the complete new
setup. Setup/domain/timestep commands cannot be smuggled into preparation.

Every new field receives fresh initialization; history receives the proposed
objects under the new integrator bindings. The old providers need not be enabled
or executable. No old buffer is passed to a new kernel. The final guarded batch
accepts component edits, lock, fresh captures and verified schemas together,
retaining inventory leases/revision protection through acceptance. Cancellation,
stale context, unavailable providers, invalid values or any initialization failure
leave the original experiment and capabilities unchanged. Undo/redo restore the
accepted old/new state without executing either set of kernels.

### Native captured replacement and binding-only changes

1. Choose an object's **Choose replacement…** and its new exact component, or
   **Load current component choices** to change bindings without changing objects.
   Configure explicit new values where applicable. The complete component choices
   start from captured/saved intent. A reset proposal permits explicit rebinding;
   review shared consumers, because a binding change can affect multiple objects.
2. Check dependencies, inspect property fields if needed, then choose **Stage
   reset choices (no document change)**. No component, schema or lock is adopted.
   The inspector moves to the complete physics proposal. Only domain/grid/timestep
   are copied; select every desired field model and integrator explicitly, using
   displayed defaults or new configuration values. Old models and state are not
   retained implicitly. The selected integrator's exact Dynamics binding determines
   which objects are integrated; other objects remain kinematic.
3. Check physics dependencies and resolve all choices. This graph includes the
   staged component roots/complete bindings, not the superseded document lock.
   Confirm the combined component/provider changes and reset of **all** fields and
   history, then **Apply component choices and reset all physics**. The exact
   displayed resolved graph is frozen for independent backend revalidation.

Consent and Apply messages identify both component and physics generations.
Changing physics values/providers invalidates consent; changing component inputs
discards the staged physics proposal as well. Old checkbox/button messages cannot
authorize newer inputs. Ordinary physics/field/history consent cannot authorize
this operation. Cancel and withdrawal of full-reset consent work through scientific
completion, retaining worker ownership until actual exit. Document/mode/inventory
changes invalidate the operation; no automatic retry occurs. Failure preserves
the original and requires new consent. Success ends the proposal and advances one
revision; undo/redo restore complete state without invoking kernels.

This native chooser replaces one selected component at a time, or changes bindings
without object edits. General multi-object migration planning and MCP remain
separate adapters over the backend. It is a reset workflow, not
selective state migration or automatic component-value conversion.

## Removing objects and components

The window's Remove object and Detach component actions explicitly prepare the
surviving root set and call the shared `SelectionLock::for_roots` operation. They
submit removal and any changed lock in one ordinary command batch. A component
still attached elsewhere keeps its root; dependencies reachable from surviving
roots keep their exact providers. An unchanged root set reuses the existing lock;
removing the final root retains an explicit empty lock, not an unlocked document.
The authority still refuses raw root-changing edits without a matching lock.

This preparation is a bounded scan of authored components plus shared graph
restriction. It requires no inventory, resolution or executable installation.
Before scientific capture it works even when the document is reopened offline
with unavailable schemas. If removal changes captured Dynamics membership, the
existing guarded history lane prepares matching integrator history and accepts
removal, lock and history together. Field state remains unchanged. Input admission
checks lock bounds and charges old/proposed locks alongside captured state;
normal authority retention and reference tracking still apply at acceptance.
Missing executables, stale context or failed initialization never accept a partial
removal. Undo/redo restore object, lock and capture together without guest calls.

## Adding a component after scientific capture

Every exact **Add component** now stages local values and dependency choices,
including a repeated use of a locked root. Captured component bindings reachable
from current roots (or a promoted captured member) are copied into the proposal;
existing scientific bindings cannot be rebound. Unavailable schemas use the same
read-only **Load property fields** path. None of these actions changes the document.

After checking dependencies, confirm **Add this component and regenerate initial
integrator history**, then choose **Add component and regenerate history**.
Consent names the exact local proposal: changing values or provider choices
invalidates it, and stale checkbox messages cannot consent to later inputs.
Cancellation or withdrawing consent prevents acceptance even after handoff to
scientific preparation. Cancellation retains worker capacity until actual exit.

The guarded `ScientificEffects::extend_components` path accepts explicit exact
attachments, a complete component lock and an authoring-context guard. It
independently revalidates the lock and selected schemas, merges it with captured
provider intent without changing any existing edge, and prepares an extended
scientific declaration set with the same kernel instances. It does not select a
new field model, integrator or provider on the author's behalf.

The shared `prepare_extended_history_edit` core independently requires every old
root/member, complete binding and kernel use to survive. It verifies retained
captures against the new declarations, folds the normal commands and checks
schema/expression/configuration coherence. Only numerical-history coherence is
deferred; the proposal exposes initialization inputs, not an adoptable snapshot.
The background lane regenerates integrator history from the proposed objects and
submits attachments, lock, new evidence/history and verified schemas together.
Field buffers, domain, timestep and compute settings remain unchanged. Stale,
cancelled or invalid completion cannot publish a schema or a partial edit. The
window transfers checked intent to this independently revalidating lane in the same
update fold, rechecking document/form/inventory identity and consent. Its polling
subscription continues through history preparation. Success ends the proposal;
failure preserves local values but requires fresh consent and an explicit retry.

This native form is ordinary component attachment, not a catalog browser or
MCP parity. Replacing an existing captured provider/kernel remains a separate
migration/reset operation. Field buffers are never interpreted under new kernels.

### Captured compound and catalog preparation

`ScientificEffects::compose_scene` accepts a bounded batch of exact component
attachments, self-contained object creations and new variable definitions, with a
complete resulting component lock. It uses the same independently verified
extension/history path. Definitions cannot overwrite existing names; setup edits,
embedded locks, removals and changes to existing variables are not this operation.
Existing providers, executable kernels, field buffers and settings must survive.
Objects, copied definitions, lock, evidence/history and schemas accept as one
guarded revision. This is a preparation API, not an implemented native batch editor.

`instantiate_template` captures the adopted immutable catalog snapshot and requires
an exact template fingerprint. Shared `kagami_session::instantiation::prepare`
materializes against the original experiment and its real allocation counters;
ordinary session instantiation uses that same helper. Rewritten definitions and
provenance are copied, never linked. Reloading the catalog after submission does
not replace the retained source; a new request against changed content must match
its fingerprint. The template must be available in that snapshot; staged schemas
do not silently promote an unavailable catalog entry. The independently verified
complete supplied lock must include the template's choices without replacing any
existing or incoming provider binding. Final document validation also checks exact
roots and agreement with the captured scientific selection.

Binding count/expression bounds apply before copying caller bindings; the effect
also bounds aggregate input text/entries, then materialized commands before history
preparation. Catalog loading/materialization retains its existing catalog limits;
these checks do not claim a process-wide allocation ceiling. No worker starts for
an unpinned request or an oversized direct proposal. Cancellation, stale document
or inventory, invalid values and initialization refusal leave source and schemas
unchanged. These APIs do not themselves supply a scientific MCP/headless adapter;
callers must collect explicit choices and consent to history regeneration before
submitting. The native single-template caller is described below.

### Native catalog creation

1. In Authoring, open **Catalog**, then **Load directory…**. A background reader
   opens the shared `CatalogAuthority`; the window/session retain its immutable
   snapshot, not a competing mutable registry. **Reload** submits a guarded catalog
   command with current schema capabilities. Both are explicit reads; no watcher,
   source-file write or experiment revision is introduced.
2. Browse pages of 24 entries. Invalid/unavailable entries and file diagnostics
   remain visible. Select a structurally valid entry, including an unavailable one
   whose dependencies need resolution. The proposal pins that exact fingerprint
   and shows the components it will copy; old row/page tokens cannot select from a
   newly loaded catalog.
3. Enter the object name, numeric SI position (metres) and velocity (metres/second).
   Parameter fields accept authored expressions and show their declared unit and
   default; **Use template default** removes an override rather than authoring an
   empty expression. Other placement expressions/orientation editing are not this
   form. Field-input identity stays stable across typing, while every accepted
   change advances the separate consent/resolver generation.
4. **Check dependencies**. The shared chooser lists actual ambiguities and permits
   explicit installed alternatives for unbound requirements. Template/document
   bindings and captured consumers are frozen; creation cannot replace their
   providers. Legacy templates can create uncaptured legacy objects, but captured
   creation requires exact component pins and never infers a migration.
5. **Create object** independently revalidates the displayed complete selection,
   leases its releases and revalidates the retained catalog content under those
   selected schemas in the background. This can resolve a previously unavailable
   template, but never promotes it just because the UI selected it. Without captured
   physics, copied definitions/components, complete lock and verified capabilities
   pass normal guarded acceptance as one undoable revision; no kernels execute.
6. With captured physics, confirm **Create this object and regenerate initial
   integrator history**, then **Create and regenerate history**. The prepared
   exact snapshot enters the independently revalidating scientific extension lane;
   fields, settings and existing providers remain unchanged. Data, schema, choices
   and new history adopt atomically, not during dependency inspection.

Edits/provider changes revoke consent and invalidate reports. Stale Apply/checkbox
messages cannot authorize newer inputs or revoke an unrelated current confirmation.
Cancel/Close, consent withdrawal and document/mode/inventory changes prevent a
pending proposal from accepting. Slots remain owned until actual reads/guest work
finish; closing does not undo an explicitly requested catalog reload. Failures
require explicit retry, with fresh captured-history consent. Reloading/removing
templates never alters created objects, their copied variables or saved provenance.

Native loading allows 32 files of at most 256 KiB, 64 documents per file and 4,096
catalog bindings. The shared loader now additionally caps directory enumeration at
16 × the receiving file limit, including ignored entries/symlinks, before retaining
unbounded path/diagnostic lists. Exhaustion refuses the directory rather than
loading a filesystem-order-dependent prefix. Parameter sources are capped at the
smaller of document and 1,024-byte native limits; coordinate fields at 64 bytes,
names at 256 bytes before final name validation. Listing/diagnostic display is
bounded. These are input/work bounds, not whole-process memory or IO deadlines.

The native flow creates one selected template per action. Catalog file editing,
arbitrary multi-object composition/migration planning, emitter recipe authoring and
MCP/headless parity are not claimed by this window. No file, workload or kernel ABI
version changes. Manual panel layout/interaction QA remains separate from message
path and renderer tests.

## Evidence and remaining work

The native-message integration tests use real installed declarations. A second
compatible provider makes an existing component ambiguous; an explicit choice
restores its schema and persists without physics initialization. Tests cover
stale inventory, an intervening document edit before the completion fold, unused
binding repair, complete-edge/budget revalidation, exact leases, offline reopen,
undo/redo and inheritance of saved choices by physics proposals.

The real Newtonian/Euler fixture now uses two bodies composed from a selected
non-default vocabulary release. The component form saves their nonempty root
lock before initialization. That intent survives capture, offline v5 reopen and
portable export; a fresh runtime with no inventory admits the result and produces
oppositely directed attractive velocities. This complements, rather than replaces,
the numerical/convergence tests of the runtime and kernels.

Native removal tests cover repeated roots, dependency members also used as roots,
last-use pruning, invalid targets, offline removal and undo/redo. The real
object/history fixture carries a component lock through export and exercises
atomic final-component removal with replacement history and unchanged fields.
Attachment tests cover initial ambiguity, schema restoration, new and shared
roots, preservation of accepted bindings, stale/cancelled completions, refusal
when required values are missing and undo/redo. Property tests exercise read-only
field loading, units/text-constraint refusal, arithmetic-source preservation,
false/empty/unset distinctions, stale/cancelled property reads, edits during Apply,
cross-form messages, receiving/aggregate bounds, offline sources and different
values on repeated uses of a locked root. Authority tests additionally
check retention refusal, capability/event atomicity and replay without schema
replacement. Replacement tests cover shared roots, promoting an existing dependency
to a root, unreachable-intent pruning, invalid values, stale/cancelled proposals,
undo/redo, offline reopen and refusal after scientific capture. A local preparation
test covers replacement at the component-count limit without reserving another slot.
Native captured-addition tests cover property loading without capability adoption,
consent invalidation, old checkbox messages, refusal without consent, cancellation
and stale edits on both sides of the asynchronous handoff, undo/redo, offline
reopen/export and distinct values on a repeated root with the same retained lock.
Catalog tests use real Newtonian/Euler kernels and exact v3 template locks: repeated
instantiation after deleted IDs, parameter overrides, catalog removal while pending,
unchanged field buffers, undo/redo, offline v5 reopen and portable export reaching a
fresh inventory-free runtime. Compound tests accept two objects and a new definition
in one revision and refuse duplicate definitions without mutation. Negative tests
cover fingerprints, expressions, cancellation, stale edits and input bounds.

Still required:

- Arbitrary compound planning and headless adapters after capture; the
  single-template native catalog workflow is now connected. Ordinary
  component attachment, uncaptured replacement and native removal are covered
  above; this is not an all-purpose composition editor or an MCP adapter.
- General multi-object replacement/compound planning beyond the native selected-
  component or binding-only reset. Selective state migration is not implemented;
  ordinary field/kernel reconfiguration never authorizes changed locked providers.
- MCP/headless command DTO/adoption integration, with the same guards and leased
  preparation. No MCP implementation was changed in this increment.
- Manual multi-provider window layout/interaction QA. Native-message tests are
  evidence of the real update/authority path, not visual inspection.

No plugin, catalog, experiment, workload or kernel ABI version changes are made
by this adapter. It consumes the existing lock and document formats.
