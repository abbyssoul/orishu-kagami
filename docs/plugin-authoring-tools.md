# Local plugin authoring tools

Status: **initial Unix source/package, CLI management and native management panel
implemented; full adapter parity remains open**. Management verifies declarations;
it does not execute or admit Wasm. Windows secure package IO, complete UI/MCP
management and component-only dependency dialogs remain implementation work. Captured
scientific authoring and selected workload export now exist through separate
adapters; the shared sandbox is never invoked by plugin management. See
[X-PLUGIN delivery](tasks/x-plugin-delivery-ledger.md).

The app-local `PluginStore::resolve_authoring` adapter returns the provider decision
and, only for a fully resolved selection, component schemas from that same verified
inventory snapshot. Those exact schemas retain original scientific roles/bindings,
defaults and property limits; the document authority and catalog enforce them.
Stale, disabled, missing or ambiguous selections return no substitute schemas.
This adapter does not mutate a document, install anything, or execute a kernel.
Unix application startup now loads this vocabulary into the document authority;
document-selection dialogs and open-document lease adoption remain open.

## Build inputs

Plugins are built outside Kagami. The initial low-level source profile uses
`plugin.json` and explicitly named regular files beneath its directory:

```json
{
  "apiVersion": "orishu.plugin-source/v1",
  "metadata": {
    "pluginId": "org.example.constants",
    "versionLabel": "development"
  },
  "contributions": [
    {
      "localId": "gravity-constants",
      "extensionPoint": "orishu.model.constants/v1",
      "path": "constants.json"
    }
  ],
  "artifacts": []
}
```

For example, `constants.json` contains a contribution payload:

```json
{
  "scientific": {
    "name": "org.example.gravity.constants",
    "version": 1,
    "requirements": [],
    "constants": [
      {
        "id": "g",
        "dimension": [3, -1, -2, 0, 0, 0, 0],
        "valueSI": 6.67430e-11,
        "meaning": "Newtonian gravitational constant"
      }
    ]
  }
}
```

Known contributions use the shared [payload schemas](../crates/orishu-plugin/schema/plugin-v1.schema.json)
as JSON. Packing validates them, emits canonical CBOR, derives root requirement
slots from their exact scientific requirements and hashes payload/artifact bytes.
Unknown well-formed extension points accept raw files as opaque contributions;
this initial source profile supplies no requirements for opaque payloads. Neither
unknown bytes nor declared Wasm artifacts execute during these operations.

Each extra kernel/input entry is `{ "path": "kernel.wasm", "mediaType":
"application/wasm" }`. Its descriptor is computed from its actual bytes.
V1 source payloads carry already-exact scientific references and kernel digests,
produced by external build tooling using the shared identity helpers.
[Source v2](plugin-source-v2.md) additionally accepts explicit local contribution
and artifact aliases, resolving them in dependency order before normal payload
validation. Forward references are allowed; missing/opaque dependencies, cycles
and duplicate aliases refuse. Independent external contracts stay exact; packing
does not consult installed providers, run a compiler or execute a guest.
Do not put mutable paths into scientific identity fields. Build leftovers not
explicitly listed in `plugin.json` are not packaged.

Directory-relative opens reject path escapes, symlink files and symlinked
descendants; source paths are never extracted from an archive or passed to a
shell. Limits apply before source acquisition, including a 1 MiB source manifest,
256 contributions, 4,096 explicit artifact entries, 256 MiB per artifact and
1 GiB aggregate artifact bytes. The archive has a separate aggregate byte limit,
so a source at an artifact limit need not fit with archive overhead. Typed list
readers refuse excess entries before deserializing them.

## Commands

Run these without opening a window or starting MCP:

```sh
kagami plugin validate ./my-plugin
kagami plugin pack ./my-plugin --output ./my-plugin.okplugin
kagami plugin inspect ./my-plugin.okplugin
kagami plugin install ./my-plugin.okplugin
kagami plugin list --all-releases
kagami plugin disable org.example.constants
kagami plugin enable org.example.constants
```

Also implemented: `update <plugin-id> <bundle>`, `set-default <plugin-id>
<release-id>`, `inspect <release-id>` and `remove <plugin-id> <release-id>
[--ack-open-references]`. Install/update accept `--expect-release <sha256:...>`.
Packing refuses an existing output atomically; it never overwrites another bundle.
The [concrete contract](plugin-contract-v1-draft.md#9-local-inventory-and-cli-r7)
defines default selection and enablement semantics.

All commands accept `--json`. Outcomes have `apiVersion:
"kagami.plugin-command/v2"`, an `ok` boolean and either `result` or bounded `error`.
V2 adds local-origin metadata to release listings and package inspection/validation
summaries; update consumers that require the former v1 response shape. Command
syntax, error codes and scientific release identities are unchanged.
Failures exit nonzero. Management commands accept `--expected-revision <integer>`;
otherwise the CLI reads a revision before submitting and still rejects a concurrent
change rather than blindly retrying. The store's nonblocking writer lock returns
`Busy` if another process owns it. Retry after inspecting current state.

Use `--directory <path>` or `KAGAMI_PLUGIN_DIR` for an explicit inventory. Otherwise
the current Unix shell uses `$XDG_DATA_HOME/kagami/plugins` or
`$HOME/.local/share/kagami/plugins`. These are local distribution paths, never
workload identity. Validate/pack and path-based inspect do not open the inventory.

## Store and reader safety

The store publishes `orishu.plugin-inventory/v2` in `index.json`: an explicit
revision and at most 256 release registrations, each with logical ID, exact
release, enablement and default flag. There is exactly one default per logical
plugin; all its releases share enablement. Logical version labels never choose a
default. The index has a 256 KiB byte ceiling. Root manifests and artifact bytes
live in separate digest-keyed directories and are independently reverified on use.
V1 remains readable with unknown origin. Read-only access does not rewrite it;
the next accepted management mutation publishes v2 with its normal revision
advance. Old binaries reject v2 rather than overwrite metadata they cannot read.

Writers take one process/cross-process lock, validate the command, stage and flush
all immutable blobs and the canonical root, then atomically replace and flush the
index and its parent. Reusing matching cached bytes also flushes the verified file
and its directory before index publication: equality alone does not prove a prior
interrupted write was durable. Reopening/creating an existing child directory
re-establishes its parent barrier too. Unreferenced staging is ignored.

An error after publication retains `IoFailure` but explicitly reports that the file
is visible and durability may be uncertain. Inspect the output/index before retrying;
do not claim such a failure proves rollback or blindly repeat the old revision.
The same rule applies to bundle output if final staging unlink or directory flush
fails after its no-overwrite link has become visible.

Linux tests now inject an error and kill a real child process at each of 79 inventory
publication checkpoints across first/additional install, update, reinstall,
enable/disable, default selection, removal and v1-to-v2 migration. Six bundle-output
checkpoints cover staging, link, unlink and directory flush. Reopen proves an intact
old or new revision, independently verifies every registered release, and exercises
explicit retry or stale-revision refusal. A regression test requires reused blob/
root file and directory barriers before the index can be published. These hooks
exist only in unit-test builds, not as production environment-variable switches.
Run `cargo test --locked -p kagami --lib plugins::inventory::durability`.

This is process-death and injected-error evidence on the tested local filesystem,
not a power-cut/storage-hardware guarantee or complete syscall-failure matrix.
Initial recursive store-root/ancestor provisioning, disk-loss/reordering simulation,
other filesystems/platforms and broader recovery policy remain separate hardening
work. Staging leftovers are ignored, not automatically garbage-collected.

Shared OS locks implement explicit release leases for open documents/runs. Removal
requires acknowledgement while a lease is held and only de-registers the release;
it does not purge cached data. Leases survive acknowledged removal and disappear
when their last owner closes or their process exits. Physical garbage collection
is not implemented.

Path-based source/bundle loading now captures local acquisition metadata outside
the release: `kind` is `local-bundle` or `local-source-directory`, and `path` is
`{"encoding":"utf8","value":"/absolute/path"}` or, for non-UTF-8 Unix paths,
`{"encoding":"unix-bytes-hex","value":"<lowercase hex>"}`. Paths are bounded to
4096 decoded bytes, absolute and NUL-free. The absolute spelling is fixed before
input acquisition, without canonicalizing parent symlinks; it records the user's
selected location, not an inode identity or trustworthy future location. Byte-only
packages and old inventories have unknown origin; nothing is fabricated.
This lossless acquisition metadata does not change the existing packaging-output
writer's requirement for a UTF-8 output filename.

Install/update atomically records the first known origin for that release
registration. An explicit reinstall may fill an unknown origin, but a different
path never silently replaces a known one. Removing and re-registering a release
starts a new registration record. Origin remains when the source is moved/deleted:
inspection reads verified cached bytes and never follows the recorded path.
CLI list/inspect and native Inspect expose it as escaped local metadata. No
automatic update, fetch or execution uses it. `pack`, retained scientific packages,
release roots and workload closure exclude it entirely. Repacking the same bytes
from different paths has exactly the same release and archive identity.

The unchanged 256 KiB total index ceiling also charges origin metadata; excessive
metadata refuses publication instead of truncating paths or dropping registrations.
Writers retain the same atomic staging/flush/revision behavior. Do not export the
local inventory as scientific provenance: it may disclose private filesystem paths.

The authority can resolve installed providers with process-only overrides without
changing the index. Its cold snapshot acquisition has a 1 GiB aggregate artifact
read ceiling; it does not load kernels per viewport frame. Native startup supports
`--plugin-directory PATH` (or `KAGAMI_PLUGIN_DIR`), `--enable-plugin ID` and
`--disable-plugin ID`. Discovery reads each release once and independently resolves
at most 256 component declarations, preserving usable vocabulary when another
contribution is dormant. Non-default enabled releases remain available to saved
pins. Discovery does not choose a field model or integrator. Unknown/conflicting
overrides, corrupt content and IO/budget errors fail before window/MCP startup;
dependency issues are reported as unavailable contributions without substitution.
The Add-component action submits declared defaults as ordinary authored values;
the authority, not the UI, decides completeness, units and constraints.
The native **Plugins** panel now lists all installed releases, inspects verified
manifest contribution names/extension points, installs local bundles, updates a
chosen logical plugin, selects defaults and changes persistent enablement. Opening
the panel or choosing **Refresh** explicitly rediscovers current availability.
Successful mutations also refresh. There is no filesystem watcher or silent retry.
Displayed process-only overrides remain in effect regardless of persistent changes.

All operations use `PluginStore`, including its expected revision, secure bounded
IO and cross-process writer lock. One background job owns capacity until actual
exit; closing the panel does not cancel an inventory write. A stale displayed
revision refuses mutation. Acceptance and subsequent refresh failure are reported
separately, so users are not told to repeat an already accepted write.
Refresh adopts schemas and model choices only from one verified revision, holding
the short-lived revision guard through adoption. A changed revision invalidates
pending scientific candidates/prepared workloads and clears unapplied physics-form
choices; it does not modify saved pins, history, dirty state, submitted immutable
bytes or an accepted worker run. Captured settings can be copied back explicitly.
Dependency problems remain unavailable contributions, not substituted providers.

Native **Remove registration…** stages an exact release and inventory revision.
It offers removal only if unused or explicit acknowledgement of open references;
neither option purges bytes or rewrites experiment pins. A default release with
alternatives still requires choosing another default first. Pack/validate remain
CLI operations. Component-only dependency dialogs, automatic watching, full UI parity and
MCP management remain work, not completed X-PLUGIN integration.

The panel's **References** action now queries the shared document authority for
the selected exact release. It distinguishes current experiment, undo/redo and
retained accepted command data (including earlier Open requests). Captured
scientific dependencies count even when no object carries a component from that
provider. This is an explicitly requested, bounded, point-in-time report for this
session only—not a filesystem scan or an acquired lease. Query exhaustion is
reported as incomplete, never as “unused.” Re-query after edits for an updated
point-in-time breakdown.

Separately, an inventory-aware window now holds live release leases for its entire
document session, independent of panel visibility. Before Open or any command
submission it holds a shared `references.lock` gate. Removal must hold that gate
exclusively under the inventory writer lock; acknowledgement cannot bypass an
incomplete scan. An edit arriving during removal is refused without mutation and
can be explicitly retried. One background reconciliation scans immutable handles
for current state, undo/redo and retained accepted requests. Only its current
generation may install exact leases and release the coarse gate. Receipt-only
changes advance this generation even without a new experiment revision. Previous
precise leases survive stale or failed scans and acknowledged de-registration.

The scan caps work at 1,000,000 records and distinct releases at 256. Exhaustion,
thread, lock or IO failure keeps removal blocked; **Recheck reference leases** or
a subsequent command explicitly starts a new generation. A genuinely absent
registration is reported separately, preserving the pin without claiming a lease.
Inventory corruption must never be reported as absence. Once history/requests
release their last reference, reconciliation releases that exact lease. The one
pending image extends old immutable-state lifetime; this is not proof of a global
memory/RSS budget. External inventory changes require explicit Refresh; no watcher
protects later reinstalls of previously missing releases until reconciliation.

These are cooperative Unix advisory locks for clients implementing this protocol.
Older binaries and direct filesystem modifications do not honor the new coarse
gate. This is not filesystem-wide reference discovery, cache garbage collection,
or a worker plugin-management protocol. Accepted workloads remain self-contained.

## Scientific dependency choices in the window

Configure physics now exposes **Check dependencies** over the shared resolver and
`PluginStore::candidate_page`. It captures exact selected model and scene-component
roots, local explicit bindings, document incarnation/revision, form generation and
inventory revision. One off-window read resolves or pages the verified inventory;
a short revision guard spans result adoption. It performs no JIT or document edit.
Stale reads are discarded, not retried; a failed read releases its slot only after
actual completion. Form/document changes invalidate reports, and each new page has
a new token so delayed candidate-index clicks cannot bind another provider.

The UI shows structured missing/disabled/incompatible/cyclic/budget diagnostics and
exact ambiguity candidates. Shared limits apply: 64 diagnostics, 32 candidates per
page, 4096 bindings and 1,000,000 resolution work. Truncation is visible, never success;
pages replace rather than accumulate candidate lists. Inventory discovery retains
its existing 1 GiB read ceiling; one read slot is not an aggregate process RSS proof.
Choose one provider and explicitly check again for transitive requirements. Numeric
edits preserve these local choices; changing models clears them. Apply revalidates
all pins before initialization and atomic scientific adoption. Captured setup pins
are preserved, not silently replaced. Independent worker admission consumes the
resulting selected workload closure without any Kagami inventory.

**Browse installed alternatives** now offers a separate explicit-choice page for
external requirements, including already-resolved bindings. The shared
`explicit_provider_page` query includes enabled compatible non-default releases;
the original `candidate_page` and automatic resolver still use only defaults and
already-explicit providers. Browsing changes neither eligibility nor defaults. The
caller must choose an exact binding and revalidate it through normal resolution.
Disabled releases and incompatible contracts are not offered. Local dependency
slots have no alternatives. Installed pages retain the same bounds and stale-token
guards, and captured pins still require an explicit new proposal to change.

This implements scientific-setup dependency selection, not component-only authoring
availability repair or MCP management.
